//! A relayed user choice answers an option question by its printed id.
//! The sender checks the stored options; the receiver checks them again and
//! closes the one question that covers all of its requirements.
use super::super::state::{self, Store};
use crate::Result;
use kah_core::intent_history::{QuestionChoice, SourceRef, is_question_id, validate_question_id};
use serde_json::{Value, json};

pub struct Relay<'a> {
    pub worker: &'a str,
    pub question: &'a str,
}
impl Relay<'_> {
    fn stored(&self) -> Result<Option<Value>> {
        let path = Store::directory(self.worker)?
            .join("questions")
            .join(format!("{}.json", self.question));
        state::read(&path)
    }
    /// Resolve `--choose` (1-based number or exact option text) to one option.
    pub fn choice(&self, option: &str) -> Result<QuestionChoice> {
        validate_question_id(self.question)?;
        let question = self.stored()?.ok_or_else(|| {
            format!(
                "STEER_QUESTION_UNKNOWN stage=arguments: session {} has no question {}",
                self.worker, self.question
            )
        })?;
        if question["status"] != "pending" {
            return Err("STEER_QUESTION_CLOSED stage=arguments: question is already closed".into());
        }
        option_choice(&question, self.question, option)
    }
    /// approve/reject stays the gate answer. A stored option question gets a
    /// clear pointer to --choose; anything else keeps the earlier gate path.
    pub fn gate_only(&self) -> Result<()> {
        if !is_question_id(self.question) {
            return Ok(());
        }
        match self.stored()? {
            Some(q) if q["kind"] != "checkpoint_approval" && has_options(&q) => Err(
                "STEER_ANSWER_KIND stage=arguments: this question has options; use --question ID --choose OPTION_NUMBER|EXACT_OPTION_TEXT".into(),
            ),
            _ => Ok(()),
        }
    }
}
fn has_options(question: &Value) -> bool {
    question["options"]
        .as_array()
        .is_some_and(|o| !o.is_empty())
}
fn option_choice(question: &Value, id: &str, option: &str) -> Result<QuestionChoice> {
    if question["kind"] == "checkpoint_approval" {
        return Err(
            "STEER_QUESTION_KIND stage=arguments: gate question; use --answer approve|reject"
                .into(),
        );
    }
    let options = question["options"]
        .as_array()
        .filter(|o| !o.is_empty())
        .ok_or("STEER_QUESTION_OPTIONS stage=arguments: question has no options to choose")?;
    let position = match option.parse::<usize>() {
        Ok(n) => n,
        Err(_) => options
            .iter()
            .position(|o| o.as_str() == Some(option))
            .map_or(0, |i| i + 1),
    };
    let text = position
        .checked_sub(1)
        .and_then(|i| options.get(i))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "STEER_QUESTION_OPTION stage=arguments: choose 1..{} or the exact option text",
                options.len()
            )
        })?;
    Ok(QuestionChoice {
        question: id.into(),
        option: position,
        text: text.into(),
    })
}
/// Receiver side. Record the relayed choice on its question and close it.
/// A repeat of the same relayed record is accepted once and changes nothing.
pub fn close(worker: &str, source: &SourceRef, text: &str) -> Result<Value> {
    let choice = source
        .question_choice
        .as_ref()
        .ok_or("REVIEW_QUESTION_CHOICE_MISSING stage=question")?;
    source.validate()?;
    let directory = Store::directory(worker)?.join("questions");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let _lock = Store::delivery_at(&directory, true)?;
    let name = format!("{}.json", choice.question);
    let mut question: Value =
        state::read(&directory.join(&name))?.ok_or("REVIEW_QUESTION_UNKNOWN stage=question")?;
    if question["kind"] == "current_rule_conflict" && question["status"] == "pending" {
        // Stored duplicates of this conflict join it before the one answer closes it.
        let worker = question["worker"].as_str().unwrap_or_default().to_owned();
        crate::observer_review::option1::rule_questions::fold(
            &directory,
            &worker,
            Some(&choice.question),
        )?;
        question =
            state::read(&directory.join(&name))?.ok_or("REVIEW_QUESTION_UNKNOWN stage=question")?;
    }
    if question["worker"] != source.session_private() {
        return Err("REVIEW_QUESTION_SCOPE stage=question".into());
    }
    if question["status"] != "pending" {
        if question["resolution"]["source"]["record"] == source.record {
            return Ok(question);
        }
        return Err("REVIEW_QUESTION_ALREADY_ANSWERED stage=question".into());
    }
    let expected = option_choice(&question, &choice.question, &choice.option.to_string())?;
    if expected != *choice {
        return Err(
            "REVIEW_QUESTION_OPTION stage=question: choice does not match stored options".into(),
        );
    }
    question["status"] = "answered".into();
    question["resolution"] = json!({"choice":{"option":choice.option,"text":choice.text},
        "requirements":covered(&question),"source":source,"verbatim":text,
        "at_ms":kah_observer::session::now_ms(),
        "authority":"Explicit relayed user choice of a stored option; no model inference"});
    super::super::source_store::save(&directory, &name, &question)?;
    Ok(question)
}
/// Every requirement id one question covers (deduped questions list several).
pub fn covered(question: &Value) -> Vec<Value> {
    question["requirements"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| vec![question["requirement"].clone()])
}
