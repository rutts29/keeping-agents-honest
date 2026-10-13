//! Host-asserted user steering. Unlike kah relay, this needs no original quote match.
use crate::{inherited::Quote, session::Store};
use kah_core::{
    intent_history::{
        GateDecision, QuestionAnswer, QuestionChoice, SourceOrigin, SourceProvider, SourceRef,
        SourceRole,
    },
    session_bundle::BundleRecord,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub relayer: String,
    pub receipt: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<QuestionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<QuestionChoice>,
}
impl Message {
    pub fn new(relayer: &str, receipt: &str, text: &str) -> Result<Self, String> {
        if relayer.is_empty()
            || relayer.len() > 32
            || !relayer
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
            || receipt.is_empty()
            || receipt.len() > 128
            || !receipt
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || text.trim().is_empty()
        {
            return Err("STEER_RELAY_ARGUMENTS stage=relayed_user".into());
        }
        Ok(Self {
            relayer: relayer.into(),
            receipt: receipt.into(),
            text: text.into(),
            supersedes: None,
            answer: None,
            choice: None,
        })
    }
    pub fn wire(&self) -> String {
        format!(
            "[kah-user] relayer={} receipt={}{}{}\n{}",
            self.relayer,
            self.receipt,
            self.supersedes
                .as_ref()
                .map(|r| format!(" supersedes={r}"))
                .unwrap_or_default(),
            self.answer
                .as_ref()
                .map(|a| format!(" question={} answer={}", a.question, a.decision.as_str()))
                .or_else(|| self.choice.as_ref().map(|c| {
                    let text = serde_json::Value::String(c.text.clone());
                    format!(" question={} choice={} option={text}", c.question, c.option)
                }))
                .unwrap_or_default(),
            self.text
        )
    }
    pub fn parse(text: &str) -> Result<Option<Self>, String> {
        let Some(rest) = text.trim_start().strip_prefix("[kah-user] ") else {
            return Ok(None);
        };
        let (header, body) = rest
            .split_once('\n')
            .ok_or("STEER_RELAY_FRAME stage=relayed_user")?;
        let (relayer, receipt) = header
            .strip_prefix("relayer=")
            .and_then(|s| s.split_once(" receipt="))
            .ok_or("STEER_RELAY_FRAME stage=relayed_user")?;
        let (receipt, answer) = receipt
            .split_once(" question=")
            .map_or((receipt, None), |(r, a)| (r, Some(a)));
        let (receipt, supersedes) = receipt
            .split_once(" supersedes=")
            .map_or((receipt, None), |(r, s)| (r, Some(s)));
        let mut message = Self::new(relayer, receipt, body)?;
        if let Some(answer) = answer {
            message.read_answer(answer)?;
        }
        if let Some(target) = supersedes {
            Self::new(relayer, target, "validate receipt")?;
            message.supersedes = Some(target.into());
        }
        Ok(Some(message))
    }
    /// `ID answer=approve|reject` or `ID choice=N option="<json string>"`.
    fn read_answer(&mut self, header: &str) -> Result<(), String> {
        let frame = || "STEER_ANSWER_FRAME stage=relayed_user".to_owned();
        let (question, rest) = header.split_once(' ').ok_or_else(frame)?;
        if let Some(decision) = rest.strip_prefix("answer=") {
            let value = QuestionAnswer {
                question: question.into(),
                decision: GateDecision::parse(decision).map_err(|e| e.to_string())?,
            };
            value.validate().map_err(|e| e.to_string())?;
            self.answer = Some(value);
            return Ok(());
        }
        let (option, text) = rest
            .strip_prefix("choice=")
            .and_then(|r| r.split_once(" option="))
            .ok_or_else(frame)?;
        let value = QuestionChoice {
            question: question.into(),
            option: option.parse().map_err(|_| frame())?,
            text: serde_json::from_str(text).map_err(|_| frame())?,
        };
        value.validate().map_err(|e| e.to_string())?;
        self.choice = Some(value);
        Ok(())
    }
    pub fn quote(&self, session: &str, provider: SourceProvider) -> Result<Quote, String> {
        let record = format!("relayed-user:{}:{}", self.relayer, self.receipt);
        let user =
            BundleRecord::new(&record, SourceRole::User, &self.text).map_err(|e| e.to_string())?;
        let mut source = SourceRef::new(
            provider,
            session,
            record,
            SourceRole::User,
            user.content_digest(),
        );
        source.origin = Some(SourceOrigin::Relayed);
        source.relayer = Some(self.relayer.clone());
        source.supersedes_receipt = self.supersedes.clone();
        source.question_answer = self.answer.clone();
        source.question_choice = self.choice.clone();
        Ok(Quote {
            receipt: self.receipt.clone(),
            source,
            text: self.text.clone(),
            dispatch_item: None,
        })
    }
    /// Called only at the primary host UserPromptSubmit hook, never for tool or
    /// assistant text. Native transcript parsing keeps the envelope a delegate;
    /// the accepted user body is introduced once through the existing inbox.
    pub fn accept(
        &self,
        base: &Path,
        session: &str,
        provider: SourceProvider,
    ) -> Result<(), String> {
        let quote = self.quote(session, provider.clone())?;
        Store::accept_inherited(base, session, provider, quote)
    }
}
