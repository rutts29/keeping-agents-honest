use super::*;

#[test]
fn missing_root_is_reported_before_provider_execution() {
    let root = env::temp_dir().join(format!("kah-missing-root-{}", std::process::id()));
    assert!(!root.exists());
    let result = invoke_codex_request(&root, CompactorConfig::default(), json!({}), "fixture");
    let error = match result {
        Ok(_) => panic!("missing root accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:?}").contains("OBSERVER_ROOT"));
}
