use kah_observer::decision_units::context;
use serde_json::json;

#[test]
fn a_decision_sees_only_its_checked_manifest_and_verbatim_authority() {
    let input = json!({
        "intent":{"goal":{"text":"Deliver both components together."},"items":[
            {"id":"keep-tests","text":"Never weaken the tests."},
            {"id":"other","text":"Unrelated historical instruction."}]},
        "event":{"head":"abc","actions":[{"text":"BROKEN_INTEGRATION"}]},
        "decision":{"requirements":{
            "parser":{"id":"parser","authority_items":["goal","keep-tests"],
                "citations":[{"source":"pass","quote":"PARSER_PASS"},
                             {"source":"timing","quote":"The worker is still working."}],
                "checks":[],"judgment":{"kind":"semantic","decision":"satisfied",
                    "reasoning":"The parser receipt satisfies its obligation.","evidence_citations":[0]},
                "applicability":{"due":false,"reasoning":"It is still mid-turn.","evidence_citations":[1]}},
            "composition":{"citations":[{"quote":"BROKEN_INTEGRATION"}]}
        }}
    });
    let state = context::decision(&input, "requirement.parser").unwrap();
    let text = state.to_string();
    assert!(text.contains("PARSER_PASS"));
    assert!(text.contains("Never weaken the tests."));
    assert!(!text.contains("BROKEN_INTEGRATION"));
    assert!(!text.contains("Unrelated historical instruction."));
    assert!(!text.contains("still working"));
    assert_eq!(state["intent"]["goal"], input["intent"]["goal"]);
    assert_eq!(state["source_manifest"]["selected_citations"], json!([0]));
    assert_eq!(state["source_manifest"]["excluded_citations"], json!([1]));

    let timing = context::decision(&input, "applicability.parser").unwrap();
    assert!(timing.to_string().contains("still working"));
    assert!(!timing.to_string().contains("PARSER_PASS"));
    assert!(timing["decision"]["requirements"]["parser"]["judgment"].is_null());
}

#[test]
fn an_invalid_manifest_or_missing_authority_never_drops_content_silently() {
    let input = json!({"intent":{"goal":"whole goal","items":[]},
        "decision":{"requirements":{"r":{"id":"r","authority_items":["absent"],
            "citations":[],"judgment":{"evidence_citations":[0]}}}}});
    assert!(context::decision(&input, "requirement.r").is_err());
    assert!(context::decision(&input, "requirement.unknown").is_err());
}
