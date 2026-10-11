use kah_observer::commit_review::{DoubtLine, classify};
use serde_json::json;

#[test]
fn production_doubt_line_is_loaded_data_and_preserves_all_answers() {
    let policy: DoubtLine = serde_json::from_value(json!({"line":0.5,"provenance":["071f34a","4fa112c"],"adjustment":"reviewer/user decision on held-out data"})).unwrap();
    let answers = vec![
        json!({"request":"a","question":"first","noul":0.43}),
        json!({"request":"b","question":"second","noul":0.50}),
    ];
    let record = classify("worker", "head", &answers, &policy).unwrap();
    assert!(record.woke);
    assert_eq!(record.maximum.as_ref().unwrap().question, "second");
    assert_eq!(record.answers.len(), 2);
    let quiet = classify("worker", "head", &answers[..1], &policy).unwrap();
    assert!(!quiet.woke);
    let mut changed = policy;
    changed.line = 0.6;
    assert!(!classify("worker", "head", &answers, &changed).unwrap().woke);
    assert!(
        classify(
            "worker",
            "head",
            &answers,
            &serde_json::from_value(json!({"line":0.5,"provenance":[],"adjustment":""})).unwrap()
        )
        .is_err()
    );
}
