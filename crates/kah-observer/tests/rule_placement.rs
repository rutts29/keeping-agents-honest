use kah_observer::commit_check::{Prepared, Query};
use serde_json::json;
use std::collections::BTreeMap;
#[test]
fn minimal_placement_moves_whole_rule_once_and_preserves_links_and_other_words() {
    let rule = "Never split one decision across multiple Jev requests.";
    let before=Prepared {state:json!({"checks":{"r1":{"rule":rule,"hunk_ids":["h1"],"permission_ids":["p1"]}},"diff_hunks":{"h1":{"file":"x.rs","text":"+whole_record();\n"}},"current_permissions":{"p1":"Original full permission words"}}),questions:BTreeMap::from([("rule.r1".into(),Query::Noul {instructions:"Does `checks.r1.hunk_ids` do what `checks.r1.rule` forbids? A change authorized by `checks.r1.permission_ids` is not a violation.".into()})])};
    let after = kah_observer::commit_check::rule_in_question(&before).unwrap();
    assert!(after.state["checks"]["r1"].get("rule").is_none());
    assert_eq!(
        after.state["checks"]["r1"]["hunk_ids"],
        before.state["checks"]["r1"]["hunk_ids"]
    );
    assert_eq!(
        after.state["current_permissions"],
        before.state["current_permissions"]
    );
    let Query::Noul { instructions } = &after.questions["rule.r1"] else {
        panic!("Noul preserved")
    };
    assert_eq!(
        instructions.as_str(),
        format!(
            "Does `checks.r1.hunk_ids` do what the rule {} forbids? A change authorized by `checks.r1.permission_ids` is not a violation.",
            json!(rule)
        )
    );
}
