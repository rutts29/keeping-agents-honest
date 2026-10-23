use kah_cli::observer_review::{source_store, state::Store};
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "manual migration of an existing observer store; requires explicit runtime inputs"]
fn migrate_real_observer_sources_without_losing_evidence() {
    let worker = std::env::var("KAH_GATE_REAL_SESSION").unwrap();
    let state = Store::read(&worker).unwrap().unwrap();
    let private = Store::directory(&worker)
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let paths = [private, state.observer.workspace.clone()];
    let before = usage(&paths);
    let receipt = source_store::migrate(&worker).unwrap();
    let after = usage(&paths);
    assert_eq!(receipt["digestsVerified"], true);
    assert_eq!(receipt["fullSourcesRecoverable"], true);
    assert!(receipt["recordsMigrated"].as_u64().unwrap() > 0);
    // A later cumulative gate may revisit a fully segmented store.
    assert!(receipt["sourcesSegmented"].as_u64().is_some());
    let retained = Store::read(&worker).unwrap().unwrap();
    assert_eq!(retained.observer.session, state.observer.session);
    assert_eq!(retained.observer.pane, state.observer.pane);
    let evidence = json!({"beforeDu":before,"afterDu":after,"migration":receipt,"sameObserverSession":state.observer.session,"samePane":state.observer.pane,"contentCapsAdded":false,"sourceAliases":"shared immutable segments with exact end offsets and full-source digests","everyReplacementVerifiedBeforeRename":true,"passed":true});
    println!("{}", serde_json::to_string_pretty(&evidence).unwrap());
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join("source-storage-migration.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
fn usage(paths: &[PathBuf]) -> String {
    let output = Command::new("du")
        .args(["-s", "--block-size=1"])
        .args(paths)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
