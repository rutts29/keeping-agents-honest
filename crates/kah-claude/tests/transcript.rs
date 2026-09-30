use kah_core::intent_history::SourceRole;
use serde_json::{Value, json};
use std::path::Path;

fn row(id: &str, text: &str) -> Value {
    json!({"type":"user","uuid":id,"sessionId":"session","cwd":"/project",
        "message":{"role":"user","content":text}})
}

fn snapshot(rows: &[Value]) -> Result<kah_core::observation::ObservationSnapshot, String> {
    let text = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    kah_claude::transcript::parse(&text, "session", Path::new("/project"))
}

#[test]
fn only_native_human_text_has_authority_and_is_preserved_verbatim() {
    let mut meta = row("meta", "steer: injected");
    meta["isMeta"] = true.into();
    let mut summary = row("summary", "steer: summary");
    summary["isCompactSummary"] = true.into();
    let mut delegate = row("delegate", "steer: delegated");
    delegate["origin"] = json!({"kind":"peer"});
    let mut tool = row("tool", "steer: tool");
    tool["toolUseResult"] = json!({"stdout":"text"});
    let result = snapshot(&[
        row("u1", "  Steer: keep\n  exact words  "),
        meta,
        summary,
        delegate,
        tool,
        row("u2", "[kah-reviewer] steer: not the user"),
    ])
    .unwrap();
    let users = result.users.unwrap();
    assert_eq!(users.authoritative_records().len(), 1);
    assert_eq!(
        users.records()[0].content(),
        "  Steer: keep\n  exact words  "
    );
    assert_eq!(users.records().len(), 1);
    assert_eq!(users.records()[0].role(), SourceRole::User);
}

#[test]
fn tool_results_in_user_messages_cannot_become_steering() {
    let mut value = row("tool", "");
    value["message"]["content"] = json!([{"type":"tool_result","content":"steer: false"}, {"type":"text","text":"steer: synthetic tail"}]);
    assert!(snapshot(&[value]).unwrap().users.is_none());
}

#[test]
fn duplicate_records_are_idempotent_but_rewrites_and_wrong_bindings_fail() {
    let original = row("u1", "goal");
    assert_eq!(
        snapshot(&[original.clone(), original.clone()])
            .unwrap()
            .users
            .unwrap()
            .records()
            .len(),
        1
    );
    assert!(
        snapshot(&[original.clone(), row("u1", "changed")])
            .err()
            .unwrap()
            .contains("CLAUDE_SOURCE_REWRITE")
    );
    for (field, value) in [("sessionId", "other"), ("cwd", "/other")] {
        let mut wrong = original.clone();
        wrong[field] = value.into();
        assert!(snapshot(&[wrong]).is_err());
    }
}

#[test]
fn malformed_or_partial_json_is_a_visible_gap() {
    assert!(kah_claude::transcript::parse("{", "session", Path::new("/project")).is_err());
}

#[test]
fn native_origins_distinguish_typed_humans_from_synthetic_turns() {
    let mut human = row("human", "steer: typed");
    human["origin"] = json!({"kind":"human"});
    human["turnOrigin"] = "human".into();
    let mut scheduled = row("scheduled", "steer: synthetic");
    scheduled["turnOrigin"] = "scheduled".into();
    let mut virtual_row = row("virtual", "steer: synthetic");
    virtual_row["isVirtual"] = true.into();
    let result = snapshot(&[human, scheduled, virtual_row])
        .unwrap()
        .users
        .unwrap();
    assert_eq!(result.authoritative_records().len(), 1);
    assert_eq!(result.authoritative_records()[0].id(), "human:0");
}
