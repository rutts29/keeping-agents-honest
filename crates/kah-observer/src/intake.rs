//! Exact user source plus a decision; explicit user controls never call a model.
use crate::{
    judge::{Answer, Judgment},
    verbatim,
};
use kah_core::intent_history::{Event, History, IntentItem, IntentKind, SourceRef, SourceRole};
use serde::{Deserialize, Serialize};
pub const IGNORE_PROBABILITY: f64 = 0.2;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Ignore,
    Review,
    Record,
}
pub fn presence(probability: f64, record: f64, ignore: f64) -> Presence {
    if probability >= record {
        Presence::Record
    } else if probability <= ignore {
        Presence::Ignore
    } else {
        Presence::Review
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decision {
    Force,
    Aside,
    Inherited {
        item: Box<IntentItem>,
    },
    Jev {
        judgment: Box<Judgment>,
    },
    Verification {
        judgment: Box<Judgment>,
    },
    Validated {
        proposal: Box<crate::steering_proposal::Proposal>,
        judgment: Box<Judgment>,
    },
    Gap {
        reason: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Input {
    pub source: SourceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_record: Option<String>,
    pub text: String,
    pub decision: Option<Decision>,
    #[serde(default)]
    pub dismissed: bool,
    #[serde(default)]
    pub promoted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_intent: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub newer_conflicts: Vec<String>,
}
impl Input {
    pub fn key(&self) -> &str {
        self.local_record.as_deref().unwrap_or(&self.source.record)
    }
    pub fn recently_ignored(&self) -> bool {
        !self.dismissed
            && !self.promoted
            && matches!(&self.decision,Some(Decision::Jev{judgment}) if matches!(judgment.answers.get("is_steering"),Some(Answer::Noul{noul}) if *noul<=IGNORE_PROBABILITY))
    }
    pub fn id(&self) -> String {
        verbatim::item_id(&self.source)
    }
    pub fn status(&self, history: Option<&History>) -> &'static str {
        if let Some(item) = history.and_then(|h| h.intent(self.id())) {
            if !item.is_active() {
                return "inactive";
            }
            return if item.review_required {
                "review_required"
            } else if !item.uncertain_supersedes.is_empty() {
                "replacement_review"
            } else {
                "active"
            };
        }
        if self.dismissed {
            return "dismissed";
        }
        match &self.decision {
            Some(Decision::Aside) => "aside",
            Some(Decision::Verification { .. }) => "verification_pending",
            Some(Decision::Gap { .. }) | None => "gap",
            _ => "ignored",
        }
    }
}
pub fn decide(
    text: &str,
    role: SourceRole,
    judge: impl FnOnce() -> Result<Judgment, String>,
) -> Result<Decision, String> {
    if role != SourceRole::User {
        return Err("OBSERVER_AUTHORITY stage=intake".into());
    }
    let text = text.trim_start();
    if text
        .get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("steer:"))
    {
        return Ok(Decision::Force);
    }
    if text
        .get(..6)
        .is_some_and(|p| p.eq_ignore_ascii_case("aside:"))
    {
        return Ok(Decision::Aside);
    }
    judge().map(|judgment| Decision::Jev {
        judgment: Box::new(judgment),
    })
}
impl Decision {
    pub fn records_intent(&self) -> bool {
        match self {
            Self::Force | Self::Inherited { .. } => true,
            Self::Jev { judgment } => {
                matches!(judgment.answers.get("is_steering"),Some(Answer::Noul{noul}) if *noul>IGNORE_PROBABILITY)
            }
            Self::Validated { proposal, judgment } => {
                proposal.directs && crate::steering_proposal::validated(proposal, judgment)
            }
            _ => false,
        }
    }
    pub fn apply(
        &self,
        history: &mut History,
        text: &str,
        source: SourceRef,
    ) -> Result<(), String> {
        match self {
            Self::Force => force(history, text, source),
            Self::Inherited { item } => {
                if item.id.as_str() != verbatim::item_id(&source)
                    || item.text != text
                    || item.introduced_by != [source]
                {
                    return Err("INHERITED_ITEM stage=intake".into());
                }
                if history.intent(item.id.as_str()).is_none() {
                    history
                        .append(Event::RequirementAdded {
                            item: item.as_ref().clone(),
                            approval: None,
                        })
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            Self::Jev { judgment } => verbatim::apply(history, text, source, judgment),
            Self::Validated { proposal, judgment } => {
                verbatim::apply_validated(history, text, source, proposal, judgment)
            }
            Self::Aside | Self::Gap { .. } | Self::Verification { .. } => Ok(()),
        }
    }
}
pub fn force(history: &mut History, text: &str, source: SourceRef) -> Result<(), String> {
    if source.role != SourceRole::User {
        return Err("OBSERVER_AUTHORITY stage=intake".into());
    }
    let id = verbatim::item_id(&source);
    if history.intent(&id).is_some() {
        return Ok(());
    }
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(id, IntentKind::Constraint, text, vec![source]),
            approval: None,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
