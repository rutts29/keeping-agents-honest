//! Delivery progress for a persistent native session. Only native primary user
//! records acknowledge a submission; automation feedback never enters this queue.
use kah_core::unattended::RunControl;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
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
