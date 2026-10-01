//! Delivery progress for a persistent native session. Only native primary user
//! records acknowledge a submission; automation feedback never enters this queue.
use kah_core::unattended::RunControl;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confirmation {
    pub instance: String,
    pub session: String,
    pub source: String,
    pub transcript: std::path::PathBuf,
    pub transcript_bytes: u64,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub herdr_socket: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_ready: Option<bool>,
    #[serde(default)]
    pub online: bool,
    pub provider: String,
    pub session: Option<String>,
    pub pane: Option<String>,
    #[serde(default)]
    pub waiting: Option<String>,
    pub source_cursor: usize,
    pub pending: VecDeque<u64>,
    pub sending: Option<u64>,
    pub submitted: VecDeque<u64>,
    pub native_tokens: u64,
    pub observer_tokens: u64,
    pub processed_digest: Option<String>,
}
impl Native {
    pub fn new(provider: &str) -> Result<Self, String> {
        if !matches!(provider, "codex" | "claude") {
            return Err("RUN_AGENT stage=run_create: choose codex or claude".into());
        }
        Ok(Self {
            name: None,
            herdr_socket: None,
            instance: None,
            confirmation: None,
            report_ready: Some(false),
            online: false,
            provider: provider.into(),
            session: None,
            pane: None,
            waiting: None,
            source_cursor: 0,
            pending: VecDeque::new(),
            sending: None,
            submitted: VecDeque::from([0]),
            native_tokens: 0,
            observer_tokens: 0,
            processed_digest: None,
        })
    }
    pub fn synchronized(&self) -> bool {
        self.pending.is_empty() && self.sending.is_none() && self.submitted.is_empty()
    }
    pub fn continuation(&self) -> Option<&'static str> {
        (self.session.is_some() && self.pending.is_empty() && self.sending.is_none()
            && (self.submitted.is_empty() || self.submitted.iter().copied().eq([0]))).then_some(
            "[kah-builder] The controller resumed this native session. Continue the user's active task from the preserved worktree and conversation. Preserve completed work and all user constraints. If already finished, report that. This is automation, not new user intent.",
        )
    }
    pub fn can_deliver(&self, initial_turn_running: bool) -> bool {
        !self.pending.is_empty()
            && (self.submitted.is_empty()
                || initial_turn_running && self.submitted.iter().copied().eq([0]))
    }
    pub fn current(&self, control: &RunControl, sequence: u64) -> bool {
        self.synchronized()
            && control
                .inputs()
                .last()
                .is_some_and(|input| input.sequence == sequence)
    }
    pub fn sources(&mut self, control: &mut RunControl, texts: &[String]) -> Result<(), String> {
        if self.source_cursor > texts.len() {
            return Err("RUN_SOURCE_TRUNCATED stage=run_observation".into());
        }
        if self.source_cursor == 0 && texts.first().is_some_and(|text| text != control.task()) {
            return Err("RUN_FIRST_PROMPT_MISMATCH stage=run_observation".into());
        }
        for text in &texts[self.source_cursor..] {
            let next = self.submitted.front().copied().or(self.sending);
            if next.is_some_and(|seq| {
                control
                    .inputs()
                    .get(seq as usize)
                    .is_some_and(|input| input.text == *text)
            }) {
                if self.submitted.front().copied() == next {
                    self.submitted.pop_front();
                } else {
                    self.pending.retain(|seq| Some(*seq) != self.sending);
                    self.sending = None;
                }
            } else {
                control.steer(text)?;
            }
            self.source_cursor += 1;
        }
        Ok(())
    }
    pub fn begin_delivery(&mut self, control: &RunControl) -> Result<Option<String>, String> {
        let Some(sequence) = self.pending.front().copied() else {
            return Ok(None);
        };
        let input = control
            .inputs()
            .get(sequence as usize)
            .ok_or("RUN_PENDING_INPUT stage=run_delivery")?;
        self.sending = Some(sequence);
        Ok(Some(input.text.clone()))
    }
    pub fn delivered(&mut self) -> Result<(), String> {
        let sequence = self
            .sending
            .take()
            .ok_or("RUN_DELIVERY_MISSING stage=run_delivery")?;
        if self.pending.pop_front() != Some(sequence) {
            return Err("RUN_DELIVERY_ORDER stage=run_delivery".into());
        }
        self.submitted.push_back(sequence);
        Ok(())
    }
    pub fn account(
        &mut self,
        control: &mut RunControl,
        native: u64,
        observer: u64,
        elapsed_ms: u64,
    ) {
        control.account(
            native.saturating_sub(self.native_tokens),
            observer.saturating_sub(self.observer_tokens),
            elapsed_ms,
        );
        self.native_tokens = self.native_tokens.max(native);
        self.observer_tokens = self.observer_tokens.max(observer);
    }
}

pub fn session_name(id: &str, task: &str) -> String {
    let slug = task
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_ascii_lowercase();
    let slug = slug.chars().take(32).collect::<String>();
    format!("kah-{id}-{}", if slug.is_empty() { "task" } else { &slug })
}
