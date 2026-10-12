use super::support;
use serde_json::json;
use std::fs;

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
