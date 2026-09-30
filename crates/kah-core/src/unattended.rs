//! Provider-neutral control of one isolated run. Model claims are not completion.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub tokens: Option<u64>,
    pub seconds: Option<u64>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Running,
    Stopping,
    Stopped,
    BudgetStopped,
    TimeStopped,
    ExternalActionRequired,
    Completed,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserInput {
    pub sequence: u64,
    pub text: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "ControlWire")]
pub struct RunControl {
    inputs: Vec<UserInput>,
    status: RunStatus,
    limits: Limits,
    tokens: u64,
    elapsed_ms: u64,
    checkpoint: Option<String>,
    external_question: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlWire {
    inputs: Vec<UserInput>,
    status: RunStatus,
    limits: Limits,
    tokens: u64,
    elapsed_ms: u64,
    checkpoint: Option<String>,
    external_question: Option<String>,
}
impl TryFrom<ControlWire> for RunControl {
    type Error = String;
    fn try_from(value: ControlWire) -> Result<Self, String> {
        if value.inputs.is_empty() {
            return Err("RUN_INPUT_MISSING stage=run_load".into());
        }
        for (index, input) in value.inputs.iter().enumerate() {
            validate_text(&input.text)?;
            if input.sequence != index as u64 {
                return Err("RUN_INPUT_SEQUENCE stage=run_load".into());
            }
        }
        if value
            .checkpoint
            .as_ref()
            .is_some_and(|id| !valid_commit(id))
        {
            return Err("RUN_COMMIT_ID stage=run_load".into());
        }
        if value.status == RunStatus::Completed && value.checkpoint.is_none() {
            return Err("RUN_COMPLETION_WITHOUT_CHECKPOINT stage=run_load".into());
        }
        if value.status == RunStatus::ExternalActionRequired && value.external_question.is_none() {
            return Err("RUN_EXTERNAL_QUESTION_MISSING stage=run_load".into());
        }
        Ok(Self {
            inputs: value.inputs,
            status: value.status,
            limits: value.limits,
            tokens: value.tokens,
            elapsed_ms: value.elapsed_ms,
            checkpoint: value.checkpoint,
            external_question: value.external_question,
        })
    }
}

/// Constructed by the controller from tests and the intent judge, never directly
/// deserialized from a worker's final message.
#[derive(Clone, Debug)]
pub struct CompletionEvidence {
    pub ledger_user_sequence: u64,
    pub commit: String,
    pub tests_passed: bool,
    pub outcomes_met: bool,
}
impl RunControl {
    pub fn new(task: &str, limits: Limits) -> Result<Self, String> {
        validate_text(task)?;
        Ok(Self {
            inputs: vec![UserInput {
                sequence: 0,
                text: task.into(),
            }],
            status: RunStatus::Created,
            limits,
            tokens: 0,
            elapsed_ms: 0,
            checkpoint: None,
            external_question: None,
        })
    }
    pub fn task(&self) -> &str {
        &self.inputs[0].text
    }
    pub fn inputs(&self) -> &[UserInput] {
        &self.inputs
    }
    pub fn status(&self) -> RunStatus {
        self.status
    }
    pub fn tokens_used(&self) -> u64 {
        self.tokens
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms
    }
    pub fn last_checkpoint(&self) -> Option<&str> {
        self.checkpoint.as_deref()
    }
    pub fn external_question(&self) -> Option<&str> {
        self.external_question.as_deref()
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.status != RunStatus::Created {
            return Err("RUN_ALREADY_STARTED stage=run_control".into());
        }
        self.activate()
    }
    pub fn resume(&mut self) -> Result<(), String> {
        if matches!(
            self.status,
            RunStatus::Running | RunStatus::Stopping | RunStatus::Created | RunStatus::Completed
        ) {
            return Err("RUN_NOT_RESUMABLE stage=run_control".into());
        }
        self.activate()
    }
    fn activate(&mut self) -> Result<(), String> {
        if let Some(status) = self.exhausted() {
            self.status = status;
            return Err("RUN_ALLOWANCE_EXHAUSTED stage=run_control: counters are preserved".into());
        }
        self.status = RunStatus::Running;
        self.external_question = None;
        Ok(())
    }
    pub fn steer(&mut self, text: &str) -> Result<u64, String> {
        validate_text(text)?;
        if self.status == RunStatus::Completed {
            return Err("RUN_COMPLETED stage=steer".into());
        }
        let sequence = self
            .inputs
            .last()
            .unwrap()
            .sequence
            .checked_add(1)
            .ok_or("RUN_SEQUENCE_EXHAUSTED stage=steer")?;
        self.inputs.push(UserInput {
            sequence,
            text: text.into(),
        });
        Ok(sequence)
    }
    pub fn account(&mut self, input: u64, output: u64, elapsed_ms: u64) {
        self.tokens = self.tokens.saturating_add(input).saturating_add(output);
        self.elapsed_ms = self.elapsed_ms.saturating_add(elapsed_ms);
        if self.status == RunStatus::Running
            && let Some(status) = self.exhausted()
        {
            self.status = status;
        }
    }
    pub fn request_stop(&mut self) {
        match self.status {
            RunStatus::Created => self.status = RunStatus::Stopped,
            RunStatus::Running => self.status = RunStatus::Stopping,
            _ => (),
        }
    }
    pub fn stop(&mut self) {
        if matches!(
            self.status,
            RunStatus::Created | RunStatus::Running | RunStatus::Stopping
        ) {
            self.status = RunStatus::Stopped;
        }
    }
    pub fn block_external(&mut self, question: &str) -> Result<(), String> {
        validate_text(question)?;
        if self.status != RunStatus::Running {
            return Err("RUN_NOT_RUNNING stage=external_action".into());
        }
        self.external_question = Some(question.into());
        self.status = RunStatus::ExternalActionRequired;
        Ok(())
    }
    pub fn checkpoint(&mut self, commit: String) -> Result<(), String> {
        if !valid_commit(&commit) {
            return Err("RUN_COMMIT_ID stage=checkpoint".into());
        }
        if self.status == RunStatus::Completed {
            return Err("RUN_COMPLETED stage=checkpoint".into());
        }
        self.checkpoint = Some(commit);
        Ok(())
    }
    pub fn complete(&mut self, proof: &CompletionEvidence) -> Result<bool, String> {
        if self.status != RunStatus::Running {
            return Ok(false);
        }
        let current = self
            .inputs
            .last()
            .ok_or("RUN_INPUT_MISSING stage=completion")?
            .sequence;
        let proven = proof.ledger_user_sequence == current
            && self.checkpoint.as_deref() == Some(&proof.commit)
            && proof.tests_passed
            && proof.outcomes_met;
        if proven {
            self.status = RunStatus::Completed;
        }
        Ok(proven)
    }
    fn exhausted(&self) -> Option<RunStatus> {
        if self.limits.tokens.is_some_and(|limit| self.tokens >= limit) {
            Some(RunStatus::BudgetStopped)
        } else if self
            .limits
            .seconds
            .is_some_and(|limit| self.elapsed_ms >= limit.saturating_mul(1000))
        {
            Some(RunStatus::TimeStopped)
        } else {
            None
        }
    }
}
fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > 64 * 1024 || text.contains('\0') {
        return Err(
            "RUN_USER_TEXT stage=run_control: text must be nonempty, bounded and NUL-free".into(),
        );
    }
    Ok(())
}

fn valid_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_hexdigit())
}
