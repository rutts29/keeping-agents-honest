//! Actual persisted-screen boundary, with local typed receipts and zero HTTP.
use kah_cli::observer_review::{option1::journal, source_store, state::Store};
use kah_observer::commit_review::{DoubtLine, classify};
use serde_json::{Value, json};
use std::fs;

#[test]
fn only_a_new_persisted_fast_screen_gets_a_real_judged_receipt() {
    std::env::var_os("XDG_STATE_HOME")
        .expect("run through demos/check-tests.sh with isolated temporary state");
    let worker = format!("fast-boundary-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let policy: DoubtLine =
        serde_json::from_str(include_str!("../../../assets/observer/commit-policy.json")).unwrap();
    for (letter, probability) in [('a', 0.04), ('b', 0.95)] {
        let head = letter.to_string().repeat(40);
        let record = classify(
            &worker,
            &head,
            &[json!({"request":"literal-receipt","question":"r","noul":probability})],
            &policy,
        )
        .unwrap();
        let path = store
            .directory
            .join("stage-timings")
            .join(format!("jev-only-{head}.json"));
        journal::save(&record).unwrap();
        assert!(
            !path.exists(),
            "historical/generic saves must never invent old times"
        );
        let before = kah_observer::session::now_ms();
        journal::save_screen(&record).unwrap();
        assert_eq!(
            journal::load(&worker, &head).unwrap().unwrap().answers[0].noul,
            probability
        );
        if record.requires_native() {
            assert!(
                !path.exists(),
                "a flagged screen still needs native confirmation"
            );
        } else {
            let timing: Value = source_store::read(&path).unwrap();
            assert_eq!(timing["worker"], worker);
            assert_eq!(timing["request"], format!("jev-only-{head}"));
            let judged = timing["at_ms"]["judged"].as_u64().unwrap();
            assert!(judged >= before && judged <= kah_observer::session::now_ms());
            journal::save_screen(&record).unwrap();
            assert_eq!(
                source_store::read::<Value>(&path).unwrap()["at_ms"]["judged"],
                judged
            );
        }
    }
    fs::remove_dir_all(store.directory.parent().unwrap()).unwrap();
}
