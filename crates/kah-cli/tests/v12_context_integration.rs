use serde_json::json;
#[test]
fn main_side_dispatch_preserves_the_complete_gate_coverage_state() {
    let complete = json!({"intent":{"goal":"round and serialize"},"event":{"head":"55911b4242f263a9d9531f08bb956a5e2f760d57","selected_host_check":{"success":true}},
        "decision":{"requirements":{"normalize":{"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Exact check asserts the delivered behavior"}}}},"source_manifest":{"checked_sources":["whole-source"]}});
    let input = json!({"verification_contracts":{"normalize":complete}});
    let actual =
        kah_observer::decision_units::context::decision(&input, "coverage_decision.normalize")
            .unwrap();
    assert_eq!(actual, complete);
}
