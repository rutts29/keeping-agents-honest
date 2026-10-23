//! Native reference: herdrdev/herdr v0.9.3 skills/herdr/SKILL.md pane list/get.
use kah_cli::observer_review::runtime::binding::present;
use serde_json::json;
#[test]
fn absent_old_observer_is_not_a_warm_pane_but_present_owner_still_needs_validation() {
    let inventory = json!({"result":{"panes":[{"pane_id":"ws1:p1"},{"pane_id":"ws1:pD"},{"pane_id":"ws1:pN"}]}});
    assert!(!present(&inventory, "ws1:pK").unwrap());
    assert!(present(&inventory, "ws1:pD").unwrap());
    assert!(present(&json!({"result":{"error":"server unavailable"}}), "ws1:pK").is_err());
    assert!(present(&json!({"result":{"panes":[{"agent":"codex"}]}}), "ws1:pK").is_err());
}
