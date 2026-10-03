use crate::judge::{Questions, questions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Item {
    pub id: String,
    pub text: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Gold {
    pub is_steering: bool,
    pub steer_kind: String,
    pub supersedes: Option<String>,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Row {
    pub id: String,
    pub session: String,
    pub ts: Value,
    pub source_kind: String,
    pub user_text: String,
    pub prev_assistant: String,
    pub prior_items: Vec<Item>,
    pub gold: Gold,
}
#[derive(Debug)]
pub struct Case {
    pub row: Row,
    pub state: Value,
    pub questions: Questions,
}
#[derive(Default)]
struct Session {
    goal: Option<Item>,
    active: Vec<Item>,
}

pub fn validation_id(id: &str) -> bool {
    matches!(id.as_bytes().last(), Some(b'0' | b'1' | b'2'))
}

/// Gold prior context is teacher-forced. Model predictions never update it.
pub fn prepare(rows: Vec<Row>, validation: bool) -> Result<Vec<Case>, String> {
    let mut sessions = BTreeMap::<String, Session>::new();
    let mut ids = BTreeSet::new();
    let mut result = vec![];
    for row in rows {
        if row.id.is_empty()
            || row.id.len() > 128
            || !ids.insert(row.id.clone())
            || !matches!(row.source_kind.as_str(), "user" | "user_question_reply")
        {
            return Err("EVAL_ROW_ID_OR_SOURCE stage=replay".into());
        }
        let session = sessions.entry(row.session.clone()).or_default();
        let prior = session.active.clone();
        if prior.len() != row.prior_items.len()
            || !prior
                .iter()
                .zip(&row.prior_items)
                .all(|(full, view)| full.id == view.id && full.text == view.text)
        {
            return Err(format!(
                "EVAL_GOLD_PRIOR_MISMATCH stage=replay id={}",
                row.id
            ));
        }
        let items = row
            .prior_items
            .iter()
            .map(|i| (i.id.clone(), i.text.clone()))
            .collect::<Vec<_>>();
        let state = questions::steering_state(
            session.goal.as_ref().map(|g| g.text.as_str()),
            session.goal.as_ref().map(|g| g.id.as_str()),
            &items,
            &row.user_text,
            &row.prev_assistant,
        );
        let questions = questions::steering(&items);
        advance(session, &row);
        if validation_id(&row.id) == validation {
            result.push(Case {
                row,
                state,
                questions,
            });
        }
    }
    Ok(result)
}
fn advance(session: &mut Session, row: &Row) {
    if !row.gold.is_steering {
        return;
    }
    if let Some(target) = row.gold.supersedes.as_deref().filter(|s| *s != "none") {
        session.active.retain(|i| i.id != target);
    }
    let item = Item {
        id: row.id.clone(),
        text: row.user_text.clone(),
    };
    if session.goal.is_none() {
        session.goal = Some(item.clone());
    }
    session.active.push(item);
}
