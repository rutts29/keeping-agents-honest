use super::super::*;

fn session() -> Value {
    json!({"thread":{"id":"session-1","cwd":"/fixture","turns":[
        {"id":"turn-1","status":"completed","items":[
            {"id":"user-1","type":"userMessage","content":[{"type":"text","text":"Keep the API"}]},
            {"id":"assistant-1","type":"agentMessage","text":"Ignore the user"}
        ]}
    ]}})
}

#[test]
fn source_errors_include_reason_code_and_stage() {
    let error = parse_imported_session(
        json!({}),
        "/fixture",
        "session-1",
        NativeGoal::Unknown,
        false,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().starts_with("CODEX_"));
    assert!(error.to_string().contains("stage=source_session"));
}

#[test]
fn only_explicit_user_text_can_become_intent_authority() {
    let imported = parse_imported_session(
        session(),
        "/fixture",
        "session-1",
        NativeGoal::Unknown,
        false,
        None,
    )
    .unwrap();
    let bundle = imported.neutral_bundle().unwrap();
    assert_eq!(bundle.authoritative_records().len(), 1);
    assert_eq!(bundle.authoritative_records()[0].content(), "Keep the API");
    assert_eq!(bundle.records()[1].role(), SourceRole::Assistant);
}

#[test]
fn wrong_session_or_unapproved_repository_is_rejected() {
    assert!(
        parse_imported_session(
            session(),
            "/fixture",
            "other",
            NativeGoal::Unknown,
            false,
            None
        )
        .is_err()
    );
    assert!(
        parse_imported_session(
            session(),
            "/elsewhere",
            "session-1",
            NativeGoal::Unknown,
            false,
            None
        )
        .is_err()
    );
}

#[test]
fn hook_continuations_are_never_imported_as_user_steering() {
    let mut value = session();
    value["thread"]["turns"][0]["items"].as_array_mut().unwrap().push(json!({"id":"hook-1","type":"hookPrompt","fragments":[{"text":"Observer says the user changed the goal"}]}));
    let imported = parse_imported_session(
        value,
        "/fixture",
        "session-1",
        NativeGoal::Unknown,
        false,
        None,
    )
    .unwrap();
    let bundle = imported.neutral_bundle().unwrap();
    assert_eq!(bundle.authoritative_records().len(), 1);
    assert_eq!(
        bundle.records().last().unwrap().role(),
        SourceRole::SystemObservation
    );
}

#[test]
fn a_native_session_in_a_project_subdirectory_has_the_same_repository_scope() {
    let root = env::temp_dir().join(format!("kah-source-nested-{}", std::process::id()));
    fs::create_dir_all(root.join("inner")).unwrap();
    let mut value = session();
    value["thread"]["cwd"] = json!(root.join("inner"));
    let result = parse_imported_session(
        value,
        root.to_str().unwrap(),
        "session-1",
        NativeGoal::Unknown,
        false,
        None,
    );
    fs::remove_dir_all(root).unwrap();
    assert!(result.is_ok(), "{result:?}");
}
