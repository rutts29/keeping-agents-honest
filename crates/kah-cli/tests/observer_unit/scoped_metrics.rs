use super::*;
use kah_observer::judge::Answer;

#[test]
fn post_ack_check_manifest_failure_keeps_report_origin_and_recovers_when_corrected() {
    use crate::observer_review::failure::{Origin, correctable, origin};
    // The thinker can select an assertion while omitting its
    // cited command/result/SHA dependency. This must never become a host gap.
    let mut input = json!({"intent":{"goal":"Check pipeline","items":[]},"event":{"head":"a".repeat(40)},"decision":{"requirements":{"pipeline":{"authority_items":["goal"],"name":"Pipeline composes modules","citations":[{"quote":"assertion"},{"quote":"command and result"}],"checks":[{"command_citation":1,"result_citation":1,"sha_citation":1}],"judgment":{"reasoning":"Checks cover composition","evidence_citations":[0]}}}}});
    let error = report_context(&input, "requirement.pipeline").unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("REVIEW_DECISION_CHECK_MANIFEST")
    );
    assert_eq!(origin(error.as_ref()), Origin::ReportValidation);
    assert!(correctable(origin(error.as_ref()), &error.to_string()));
    input["decision"]["requirements"]["pipeline"]["judgment"]["evidence_citations"] = json!([0, 1]);
    report_context(&input, "requirement.pipeline").unwrap();
}

#[test]
fn retries_preserve_cached_answers_without_reporting_new_usage() {
    let mut total = Judgment {
        backend: "not_requested".into(),
        model: "none".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    let mut first = total.clone();
    first.backend = "jev".into();
    first.model = "jev-latest".into();
    first
        .answers
        .insert("requirement.parser".into(), Answer::Noul { noul: 0.99 });
    first.metrics.calls = 1;
    first.metrics.input_tokens = 120;
    merge(&mut total, &first, true).unwrap();
    assert!(total.answers.contains_key("requirement.parser"));
    assert_eq!(total.metrics.calls, 0);
    let mut second = first.clone();
    second.answers.clear();
    second.answers.insert(
        "requirement.composition".into(),
        Answer::Noul { noul: 0.98 },
    );
    merge(&mut total, &second, false).unwrap();
    assert_eq!(total.answers.len(), 2);
    assert_eq!(total.metrics.calls, 1);
    assert_eq!(total.metrics.input_tokens, 120);
    assert!(
        merge(&mut total, &second, true)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_DECISION_DUPLICATE")
    );
}
