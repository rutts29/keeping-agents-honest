//! New authority wakes the thinker; code never guesses which option it answers.
use super::super::state::Store;
use crate::Result;
use kah_core::run_store::RunStore;
use serde_json::{Value, json};

pub fn queue(store: &Store, worker: &str) -> Result<()> {
    if super::pending(&store.directory)?.is_empty() {
        return Ok(());
    }
    let source = kah_observer::session::Store::read(&RunStore::default_state_root()?, worker)?
        .ok_or("REVIEW_PRIMARY_STATE stage=question")?;
    let Some(projection) = kah_observer::engine::projection(&source)? else {
        return Ok(());
    };
    let intent = json!({"goal":projection.goal.text,"goal_source":projection.goal.source_refs,"items":projection.preserve.iter().chain(&projection.frontier).chain(&projection.validation).map(|i|json!({"id":i.id,"text":i.text,"source":i.introduced_by})).collect::<Vec<_>>()});
    let _delivery = Store::delivery_at(&store.directory.join("questions"), true)?;
    for mut question in super::pending(&store.directory)? {
        if question["kind"] == "checkpoint_approval" {
            continue;
        }
        let items = super::new_user_items(&question, &intent);
        let digest = kah_observer::context_selection::digest(&json!(items));
        if items.is_empty() || question["last_answer_sources"] == digest {
            continue;
        }
        let request = super::super::workspace::request(
            store,
            question["request"]
                .as_str()
                .ok_or("REVIEW_QUESTION_REQUEST stage=question")?,
        )?;
        let mut trigger = request.trigger;
        trigger["question_answer_review"] = json!({"question":question,"new_authority":items});
        super::super::queue::enqueue(worker, request.purpose, &request.turn, trigger)?;
        question["last_answer_sources"] = Value::String(digest);
        super::super::source_store::save(
            &store.directory.join("questions"),
            &format!("{}.json", question["id"].as_str().unwrap()),
            &question,
        )?;
    }
    Ok(())
}
