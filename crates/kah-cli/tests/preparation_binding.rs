use kah_cli::observer_review::{preparation, state::Store};
use serde_json::json;
use std::fs;
#[test]
fn secondary_decision_preparation_stays_bound_to_its_owning_review() {
    let worker = format!("preparation-owner-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    assert_eq!(
        preparation::active_decision(&worker, "review-parent").unwrap(),
        "review-parent"
    );
    kah_cli::unattended_report::save(
        &store.directory,
        "preparation-current.json",
        &json!({"review":"review-parent","decision":"review-parent-conflict"}),
    )
    .unwrap();
    assert_eq!(
        preparation::active_decision(&worker, "review-parent").unwrap(),
        "review-parent-conflict"
    );
    assert_eq!(
        preparation::active_decision(&worker, "review-next").unwrap(),
        "review-next"
    );
    let directory = store.directory.clone();
    drop(store);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
