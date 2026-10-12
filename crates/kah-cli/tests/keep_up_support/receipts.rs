use super::support;
use serde_json::json;
use std::fs;

#[test]
fn a_persisted_fast_verdict_is_measured_at_its_actual_host_receipt() {
    use kah_cli::observer_review::{option1::journal, source_store, state::Store};
    use kah_observer::commit_review::{DoubtLine, classify};
    std::env::var_os("XDG_STATE_HOME").expect("private check-tests state required");
    let worker = format!("keep-up-fast-boundary-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    for dir in ["queued", "requests"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    source_store::save(
        &root,
        "state.json",
        &json!({"pending":null,"observer":{},"worker":{}}),
    )
    .unwrap();
    let head = "a".repeat(40);
    let policy: DoubtLine = serde_json::from_str(include_str!(
        "../../../../assets/observer/commit-policy.json"
    ))
    .unwrap();
    let record = classify(
        &worker,
        &head,
        &[json!({"request":"real-boundary-receipt","question":"r","noul":0.04})],
        &policy,
    )
    .unwrap();
    let before = kah_observer::session::now_ms();
    journal::save_screen(&record).unwrap();
    let after = kah_observer::session::now_ms();
    let timing: serde_json::Value = source_store::read(
        &root
            .join("stage-timings")
            .join(format!("jev-only-{head}.json")),
    )
    .unwrap();
    let sample = support::sample(&root, &root);
    let row = sample["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["head"] == head)
        .unwrap();
    assert_eq!(row["at_ms"]["judged"], timing["at_ms"]["judged"]);
    let judged = row["at_ms"]["judged"].as_u64().unwrap();
    assert!(judged >= before && judged <= after);
    assert_eq!(row["missing_stages"], json!([]));
    assert_eq!(
        row["stage_sources"]["judged"],
        "per-commit persisted Jev-only verdict receipt"
    );
    assert_eq!(
        row["answer_receipts"][0]["request"],
        "real-boundary-receipt"
    );
    drop(store);
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn jev_only_journal_receipts_are_visible_without_inventing_judgment_time() {
    let root = fixture();
    fs::create_dir(root.join("commit-reviews")).unwrap();
    let head = "a".repeat(40);
    let receipt = root.join("commit-reviews").join(format!("{head}.json"));
    fs::write(&receipt, json!({"worker":"worker","head":head,"woke":false,"native":null,
        "gaps":[],"answers":[{"request":"literal-body-1","question":"full state stays private","noul":0.04}]}).to_string()).unwrap();
    let original = fs::read(&receipt).unwrap();
    let result = support::sample(&root, &root);
    let row = result["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["head"] == head)
        .expect("Jev-only commits belong to the measured cohort");
    assert_eq!(row["receipt"], receipt.to_str().unwrap());
    assert_eq!(row["answer_receipts"][0]["request"], "literal-body-1");
    assert_eq!(row["verdict_path"], "jev_only");
    assert!(row["at_ms"]["judged"].is_null());
    assert_eq!(row["missing_stages"], json!(["judged"]));
    assert!(!result.to_string().contains("full state stays private"));
    assert_eq!(fs::read(&receipt).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn option1_class_and_actual_consumption_receipts_survive_sampling() {
    let root = fixture();
    let before = fs::read(root.join("requests/review-commit.json")).unwrap();
    let result = support::sample(&root, &root);
    let rows = result["reviews"].as_array().unwrap();
    for (id, class, consumed) in [
        ("review-commit", "commit", json!(123)),
        ("review-gate", "gate", json!(456)),
        ("review-legacy", "commit", json!(789)),
        ("review-missing", "commit", serde_json::Value::Null),
    ] {
        let row = rows.iter().find(|row| row["id"] == id).unwrap();
        assert_eq!(row["review_class"], class);
        assert_eq!(row["at_ms"]["consumed"], consumed);
        assert!(
            row["at_ms"]["judged"].is_null(),
            "no invented judgment receipt"
        );
    }
    assert!(!result.to_string().contains("must not enter measurement"));
    assert_eq!(
        fs::read(root.join("requests/review-commit.json")).unwrap(),
        before
    );
    fs::remove_dir_all(root).unwrap();
}
fn fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keep-up-receipts-{}",
        kah_cli::relay_store::nonce()
    ));
    for dir in ["requests", "queued", "stage-timings"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(
        root.join("state.json"),
        json!({"pending":null,"observer":{},"worker":{}}).to_string(),
    )
    .unwrap();
    for (id, class, stages) in [
        (
            "review-commit",
            "commit",
            json!({"consumed":123,"completed":999}),
        ),
        ("review-gate", "gate", json!({"consumed":456})),
        ("review-legacy", "commit", json!({"completed":789})),
        ("review-missing", "commit", json!({})),
    ] {
        fs::write(
            root.join("requests").join(format!("{id}.json")),
            json!({
            "id":id,"purpose":"checkpoint", "trigger":{"option1":{
                "stage":"confirmation", "original_trigger":{"review_class":class},
                "input":"must not enter measurement".repeat(1000)
            }}})
            .to_string(),
        )
        .unwrap();
        fs::write(
            root.join("stage-timings").join(format!("{id}.json")),
            json!({"at_ms":stages}).to_string(),
        )
        .unwrap();
    }
    root
}
