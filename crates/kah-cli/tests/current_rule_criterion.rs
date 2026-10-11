use kah_cli::observer_review::{
    option1::{prompt, protocol},
    state::{Purpose, Request},
};
use serde_json::json;
#[test]
fn retained_attempt_four_rebuild_prompt_and_schema_include_the_approved_criterion() {
    let input: kah_core::run_store::RuleRebuild = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/current-rule-set-glue/attempt-4-rebuild-input.json"
    ))
    .unwrap();
    assert_eq!(input.input.revision, 3);
    let req = Request {
        id: "review-criterion".into(),
        worker: "fixture-worker".into(),
        purpose: Purpose::Steering,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({"option1":{"stage":"current_rules","input_digest":input.input.digest().unwrap(),"rebuild":input}}),
        documents: vec![],
        created_ms: 0,
    };
    let criterion = "a rule is diff-checkable when the code change alone can break it. Runtime measurements, investigations, sequencing and outside-design references are not diff-checkable";
    let p = prompt::compose(
        &req,
        std::path::Path::new("/inputs"),
        std::path::Path::new("/out/report.json"),
        "bin/kah observe report",
    )
    .unwrap();
    assert!(p.contains(criterion));
    assert_eq!(
        protocol::schema("current_rules")["checkability_criterion"],
        criterion
    );
    // This checks instruction wiring on real retained input, not model judgment.
}
