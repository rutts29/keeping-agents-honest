use super::*;
use kah_observer::judge::Answer;

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
