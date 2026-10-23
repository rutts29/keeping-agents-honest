use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

mod types;
use types::*;
pub use types::{ClosureError, Ledger, LedgerError, RequirementStatus, Snapshot};

impl Ledger {
    pub fn create<P, G, I, ID, T>(path: P, goal: G, requirements: I) -> Result<Self, LedgerError>
    where
        P: AsRef<Path>,
        G: Into<String>,
        I: IntoIterator<Item = (ID, T)>,
        ID: Into<String>,
        T: Into<String>,
    {
        let goal = non_empty(goal.into(), "goal")?;
        let requirements = requirements
            .into_iter()
            .map(|(id, text)| {
                Ok(Requirement {
                    id: non_empty(id.into(), "requirement id")?,
                    text: non_empty(text.into(), "requirement text")?,
                })
            })
            .collect::<Result<Vec<_>, LedgerError>>()?;
        validate_unique_requirements(&requirements)?;
        if requirements.is_empty() {
            return Err(LedgerError::new(
                "REQUIREMENTS_EMPTY",
                "at least one requirement is required",
            ));
        }

        let ledger = Self {
            path: path.as_ref().to_path_buf(),
        };
        let record = Record {
            schema_version: SCHEMA_VERSION,
            sequence: 1,
            event: Event::RunStarted { goal, requirements },
        };
        ledger.create_with_record(&record)?;
        Ok(ledger)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let ledger = Self {
            path: path.as_ref().to_path_buf(),
        };
        ledger.records()?;
        Ok(ledger)
    }

    pub fn steer(&mut self, requirement: impl Into<String>) -> Result<String, LedgerError> {
        let requirement = non_empty(requirement.into(), "steering requirement")?;
        self.locked_append::<_, LedgerError>(|state| {
            ensure_open(state)?;
            let mut suffix = state.requirements.len() + 1;
            let id = loop {
                let candidate = format!("R{suffix}");
                if !state.requirements.contains_key(&candidate) {
                    break candidate;
                }
                suffix += 1;
            };
            let event = Event::SteeringAdded {
                requirement: Requirement {
                    id: id.clone(),
                    text: requirement,
                },
            };
            Ok(Some((event, id)))
        })?
        .ok_or_else(|| LedgerError::new("LEDGER_CLOSED", "run is already closed"))
    }

    pub fn satisfy(
        &mut self,
        requirement_id: &str,
        evidence: impl Into<String>,
    ) -> Result<(), LedgerError> {
        let requirement_id = non_empty(requirement_id.to_owned(), "requirement id")?;
        let evidence = non_empty(evidence.into(), "evidence")?;
        self.locked_append::<_, LedgerError>(|state| {
            ensure_open(state)?;
            if !state.requirements.contains_key(&requirement_id) {
                return Err(LedgerError::new(
                    "REQUIREMENT_UNKNOWN",
                    format!("unknown requirement: {requirement_id}"),
                ));
            }
            let event = Event::RequirementSatisfied {
                requirement_id,
                evidence,
            };
            Ok(Some((event, ())))
        })?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Snapshot, LedgerError> {
        let state = self.fold()?;
        let requirements: Vec<RequirementStatus> = state
            .requirements
            .into_iter()
            .map(|(id, requirement)| RequirementStatus {
                id,
                text: requirement.text,
                evidence: requirement.evidence,
            })
            .collect();
        let unresolved_requirement_ids = requirements
            .iter()
            .filter(|requirement| requirement.evidence.is_none())
            .map(|requirement| requirement.id.clone())
            .collect();
        Ok(Snapshot {
            goal: state.goal,
            requirements,
            unresolved_requirement_ids,
            closed: state.closed,
        })
    }

    pub fn close(&mut self) -> Result<(), ClosureError> {
        self.locked_append(|state| {
            if state.closed {
                return Ok(None);
            }
            let unresolved: Vec<String> = state
                .requirements
                .iter()
                .filter(|(_, requirement)| requirement.evidence.is_none())
                .map(|(id, _)| id.clone())
                .collect();
            if unresolved.is_empty() {
                Ok(Some((Event::RunClosed, ())))
            } else {
                Err(ClosureError::UnresolvedRequirements(unresolved))
            }
        })?;
        Ok(())
    }

    fn create_with_record(&self, record: &Record) -> Result<(), LedgerError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)?;
        write_record(&mut file, record)?;
        file.sync_data()?;
        Ok(())
    }

    /// One mutation under an exclusive lock on the ledger file: repair an
    /// unfinished last line, validate against the folded state, pick the next
    /// sequence number, append and sync. `plan` returns the event to append
    /// (with a value for the caller), or `None` when there is nothing to add.
    /// The lock also keeps a second writer from trimming a record this one
    /// has just acknowledged.
    fn locked_append<T, E: From<LedgerError>>(
        &self,
        plan: impl FnOnce(&FoldedState) -> Result<Option<(Event, T)>, E>,
    ) -> Result<Option<T>, E> {
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&self.path)
            .map_err(LedgerError::from)?;
        file.lock().map_err(LedgerError::from)?;
        let bytes = fs::read(&self.path).map_err(LedgerError::from)?;
        let keep = committed_prefix(&bytes).len();
        if keep != bytes.len() {
            file.set_len(keep as u64).map_err(LedgerError::from)?;
            file.sync_data().map_err(LedgerError::from)?;
        }
        let records = self.records()?;
        let count = records.len();
        let state = Self::fold_records(records)?;
        let Some((event, value)) = plan(&state)? else {
            return Ok(None);
        };
        let sequence = u64::try_from(count)
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or_else(|| LedgerError::new("SEQUENCE_EXHAUSTED", "ledger sequence exhausted"))?;
        let record = Record {
            schema_version: SCHEMA_VERSION,
            sequence,
            event,
        };
        write_record(&mut file, &record)?;
        file.sync_data().map_err(LedgerError::from)?;
        Ok(Some(value))
    }

    fn fold(&self) -> Result<FoldedState, LedgerError> {
        Self::fold_records(self.records()?)
    }

    fn fold_records(records: Vec<Record>) -> Result<FoldedState, LedgerError> {
        let mut records = records.into_iter();
        let first = records
            .next()
            .ok_or_else(|| LedgerError::new("LEDGER_EMPTY", "ledger is empty"))?;
        let Event::RunStarted { goal, requirements } = first.event else {
            return Err(LedgerError::new(
                "START_MISSING",
                "ledger must begin with run_started",
            ));
        };
        validate_unique_requirements(&requirements)?;
        let mut state = FoldedState {
            goal,
            requirements: requirements
                .into_iter()
                .map(|requirement| {
                    (
                        requirement.id,
                        RequirementState {
                            text: requirement.text,
                            evidence: None,
                        },
                    )
                })
                .collect(),
            closed: false,
        };

        for record in records {
            apply_record(&mut state, record)?;
        }
        Ok(state)
    }

    fn records(&self) -> Result<Vec<Record>, LedgerError> {
        let bytes = fs::read(&self.path)?;
        let contents = std::str::from_utf8(committed_prefix(&bytes)).map_err(|error| {
            LedgerError::new("LEDGER_JSON", format!("invalid ledger text: {error}"))
        })?;
        if contents.is_empty() {
            return Err(LedgerError::new("LEDGER_EMPTY", "ledger is empty"));
        }

        let mut records = Vec::new();
        for (index, line) in contents.lines().enumerate() {
            let record: Record = serde_json::from_str(line).map_err(|error| {
                LedgerError::new(
                    "LEDGER_JSON",
                    format!("invalid ledger line {}: {error}", index + 1),
                )
            })?;
            let expected = u64::try_from(index + 1)
                .map_err(|_| LedgerError::new("SEQUENCE_EXHAUSTED", "ledger sequence exhausted"))?;
            if record.schema_version != SCHEMA_VERSION {
                return Err(LedgerError::new(
                    "SCHEMA_UNSUPPORTED",
                    format!("unsupported schema version: {}", record.schema_version),
                ));
            }
            if record.sequence != expected {
                return Err(LedgerError::new(
                    "SEQUENCE_INVALID",
                    format!(
                        "invalid ledger sequence: expected {expected}, found {}",
                        record.sequence
                    ),
                ));
            }
            records.push(record);
        }
        Ok(records)
    }
}

fn non_empty(value: String, field: &str) -> Result<String, LedgerError> {
    if value.trim().is_empty() {
        Err(LedgerError::new(
            "VALUE_EMPTY",
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(value)
    }
}

fn validate_unique_requirements(requirements: &[Requirement]) -> Result<(), LedgerError> {
    let mut seen = BTreeMap::new();
    for requirement in requirements {
        if seen.insert(&requirement.id, ()).is_some() {
            return Err(LedgerError::new(
                "REQUIREMENT_DUPLICATE",
                format!("duplicate requirement id: {}", requirement.id),
            ));
        }
    }
    Ok(())
}

fn ensure_open(state: &FoldedState) -> Result<(), LedgerError> {
    if state.closed {
        Err(LedgerError::new("LEDGER_CLOSED", "run is already closed"))
    } else {
        Ok(())
    }
}

fn write_record(file: &mut fs::File, record: &Record) -> Result<(), LedgerError> {
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    file.write_all(&line)?;
    file.flush()?;
    Ok(())
}

/// The bytes up to and including the last newline: the committed records.
fn committed_prefix(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(&[][..], |last| &bytes[..=last])
}

fn apply_record(state: &mut FoldedState, record: Record) -> Result<(), LedgerError> {
    if state.closed {
        return Err(LedgerError::new(
            "EVENT_AFTER_CLOSURE",
            "ledger contains events after closure",
        ));
    }
    match record.event {
        Event::RunStarted { .. } => {
            return Err(LedgerError::new(
                "START_REPEATED",
                "run_started may only appear first",
            ));
        }
        Event::SteeringAdded { requirement } => {
            if state.requirements.contains_key(&requirement.id) {
                return Err(LedgerError::new(
                    "REQUIREMENT_DUPLICATE",
                    format!("duplicate requirement id: {}", requirement.id),
                ));
            }
            state.requirements.insert(
                requirement.id,
                RequirementState {
                    text: requirement.text,
                    evidence: None,
                },
            );
        }
        Event::RequirementSatisfied {
            requirement_id,
            evidence,
        } => {
            let requirement = state.requirements.get_mut(&requirement_id).ok_or_else(|| {
                LedgerError::new(
                    "REQUIREMENT_UNKNOWN",
                    format!("unknown requirement: {requirement_id}"),
                )
            })?;
            requirement.evidence = Some(evidence);
        }
        Event::RunClosed => state.closed = true,
    }
    Ok(())
}
