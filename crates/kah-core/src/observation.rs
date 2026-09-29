//! Provider-neutral wake signals and context injection. Signals carry no
//! steering authority: intent comes only from validated provider source records.
use crate::{
    intent_history::SourceRole,
    session_bundle::{BundleRecord, SessionBundleError},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Started,
    Resumed,
    Compacted,
    UserMessage,
    TurnEnded,
    Ended,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "EventWire")]
pub struct SessionEvent {
    session: String,
    event: String,
    pub kind: EventKind,
    turn: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventWire {
    session: String,
    event: String,
    kind: EventKind,
    turn: Option<String>,
}
impl TryFrom<EventWire> for SessionEvent {
    type Error = String;
    fn try_from(value: EventWire) -> Result<Self, Self::Error> {
        Self::new(
            &value.session,
            &value.event,
            value.kind,
            value.turn.as_deref(),
        )
    }
}

impl SessionEvent {
    pub fn new(
        session: &str,
        event: &str,
        kind: EventKind,
        turn: Option<&str>,
    ) -> Result<Self, String> {
        validate_id(session)?;
        validate_id(event)?;
        if let Some(turn) = turn {
            validate_id(turn)?;
        }
        if matches!(kind, EventKind::UserMessage | EventKind::TurnEnded) && turn.is_none() {
            return Err(
                "OBSERVATION_TURN stage=event: turn-scoped events require a turn ID".into(),
            );
        }
        Ok(Self {
            session: session.into(),
            event: event.into(),
            kind,
            turn: turn.map(str::to_owned),
        })
    }
    pub fn session(&self) -> &str {
        &self.session
    }
    pub fn event(&self) -> &str {
        &self.event
    }
    pub fn turn(&self) -> Option<&str> {
        self.turn.as_deref()
    }
}

fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err("OBSERVATION_ID stage=event: invalid provider event identity".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionKind {
    Continuity,
    DriftReminder,
    GapNotice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "InjectionWire")]
pub struct ContextInjection {
    pub revision: u64,
    pub kind: InjectionKind,
    pub text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InjectionWire {
    revision: u64,
    kind: InjectionKind,
    text: String,
}
impl TryFrom<InjectionWire> for ContextInjection {
    type Error = String;
    fn try_from(value: InjectionWire) -> Result<Self, Self::Error> {
        Self::new(value.revision, value.kind, value.text)
    }
}

impl ContextInjection {
    pub fn new(
        revision: u64,
        kind: InjectionKind,
        text: impl Into<String>,
    ) -> Result<Self, String> {
        let text = text.into();
        if text.trim().is_empty() || text.len() > 16 * 1024 {
            return Err(
                "OBSERVATION_CONTEXT stage=injection: context must be nonempty and bounded".into(),
            );
        }
        Ok(Self {
            revision,
            kind,
            text,
        })
    }
    pub fn as_record(&self, id: &str) -> Result<BundleRecord, SessionBundleError> {
        BundleRecord::new(id, SourceRole::SystemObservation, &self.text)
    }
}
