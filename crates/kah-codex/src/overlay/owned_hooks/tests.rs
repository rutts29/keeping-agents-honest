use super::*;
#[test]
fn duplicate_user_command_is_not_enabled_or_trusted_as_an_owned_hook() {
    let path = Path::new("/overlay/hooks.json");
    let entries = [
        json!({"key":"user","sourcePath":path,"command":"kah-hook","eventName":"sessionStart","currentHash":"user-hash"}),
        json!({"key":"owned","sourcePath":path,"command":"kah-hook","eventName":"sessionStart","currentHash":"owned-hash"}),
    ];
    let old = Table::from_iter([(
        "user".into(),
        Value::try_from(json!({"enabled":false})).unwrap(),
    )]);
    let owned = [OwnedHook {
        event: "SessionStart",
        api_event: "sessionStart",
        command: "kah-hook".into(),
        timeout: 2,
    }];
    let state = trusted_entries(&entries, path, &owned, &old).unwrap();
    assert_eq!(state["user"]["enabled"].as_bool(), Some(false));
    assert!(state["user"].get("trusted_hash").is_none());
    assert_eq!(state["owned"]["trusted_hash"].as_str(), Some("owned-hash"));
}
#[test]
fn a_changed_user_definition_loses_stale_trust() {
    let path = Path::new("/overlay/hooks.json");
    let entries = [
        json!({"key":"user","sourcePath":path,"command":"changed-hook","eventName":"sessionStart","currentHash":"new"}),
    ];
    let old = Table::from_iter([(
        "user".into(),
        Value::try_from(json!({"enabled":true,"trusted_hash":"old"})).unwrap(),
    )]);
    let state = trusted_entries(&entries, path, &[], &old).unwrap();
    assert!(state["user"].get("trusted_hash").is_none());
}
