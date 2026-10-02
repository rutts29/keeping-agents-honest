//! Host-asserted user steering. Unlike kah relay, this needs no original quote match.
use crate::{inherited::Quote, session::Store};
use kah_core::{
    intent_history::{SourceOrigin, SourceProvider, SourceRef, SourceRole},
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
            || text.len() > 16 * 1024
        {
            return Err("STEER_RELAY_ARGUMENTS stage=relayed_user".into());
        }
        Ok(Self {
            relayer: relayer.into(),
            receipt: receipt.into(),
            text: text.into(),
        })
    }
    pub fn wire(&self) -> String {
        format!(
            "[kah-user] relayer={} receipt={}\n{}",
            self.relayer, self.receipt, self.text
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
        Self::new(relayer, receipt, body).map(Some)
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
