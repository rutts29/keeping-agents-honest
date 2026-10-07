//! Grouped gate risks retain their controller ownership when referenced by an item.
use super::*;
pub(crate) fn matches(question: &Value, request: &Request, requirement: &str) -> bool {
    if question["worker"] != request.worker {
        return false;
    }
    if question["kind"] != "checkpoint_approval" {
        return question["requirement"] == requirement;
    }
    question["head"] == json!(request.head)
        && question["intent_revision"] == request.intent["revision"]
        && question["unverified_risks"]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i["requirement_id"] == requirement))
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/grouped_question_scope.rs"]
mod tests;
