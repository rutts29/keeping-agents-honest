use kah_observer::judge::{
    self, Answer, Judge, Question, Questions,
    jev::{Client, Jev},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
struct Fake;
impl Client for Fake {
    fn post(&self, body: &Value) -> Result<Value, judge::Error> {
        assert_eq!(body["model"], "jev-latest");
        assert_eq!(body["questions"]["steering"]["type"], "noul");
        assert!(body.get("api_key").is_none());
        Ok(
            json!({"model":"jev-1.13.0","answers":{"steering":{"type":"noul","noul":0.98}},"usage":{"input_tokens":123,"output_tokens":10}}),
        )
    }
}
#[test]
fn fake_client_uses_documented_wire_shape_and_preserves_noul_probability() {
    let judge = Jev {
        client: Fake,
        model: "jev-latest".into(),
    };
    let questions = Questions::from([(
        "steering".into(),
        Question::Noul {
            instructions: "Does the user add an explicit constraint?".into(),
        },
    )]);
    let result = judge
        .evaluate(&json!({"user":"Use snake_case"}), &questions)
        .unwrap();
    assert!(matches!(result.answers["steering"],Answer::Noul{noul} if noul==0.98));
    assert_eq!(result.metrics.input_tokens, 123);
    assert!(result.metrics.estimated_cost_usd.unwrap() > 0.0);
}
#[test]
fn mismatched_answers_and_invalid_distributions_are_rejected() {
    let questions = Questions::from([(
        "kind".into(),
        Question::Choice {
            instructions: "Select alignment.".into(),
            criteria: BTreeMap::from([
                ("aligned".into(), "Follows intent".into()),
                ("drift".into(), "Violates intent".into()),
            ]),
        },
    )]);
    let answers=serde_json::from_value(json!({"kind":{"type":"choice","choice":"drift","probabilities":{"aligned":0.9,"drift":0.1},"confidence":0.8}})).unwrap();
    assert_eq!(
        judge::validate_response(&questions, &answers)
            .unwrap_err()
            .code,
        "JUDGE_CHOICE"
    );
    assert_eq!(
        judge::validate_response(&questions, &BTreeMap::new())
            .unwrap_err()
            .code,
        "JUDGE_ANSWER_KEYS"
    );
    assert!(judge::validate_request(&json!("x".repeat(65537)), &questions).is_err());
}

#[test]
fn domain_defaults_and_low_confidence_never_trigger_a_reminder() {
    use kah_observer::judge::decision::AssessmentState;
    use kah_observer::judge::{Judgment, Metrics, decision, routing::Backend};
    assert_eq!(Backend::for_domain("mine", None).unwrap(), Backend::Jev);
    for domain in ["research", "untrusted"] {
        assert_eq!(Backend::for_domain(domain, None).unwrap(), Backend::Luna);
        assert_eq!(
            Backend::for_domain(domain, Some("jev")).unwrap(),
            Backend::Jev
        );
    }
    let mut judgment=Judgment{backend:"fake".into(),model:"test".into(),answers:serde_json::from_value(json!({
        "alignment":{"type":"choice","choice":"accidental_drift","confidence":0.6,"probabilities":{"accidental_drift":0.8,"unclear":0.2}},
        "drift_severity":{"type":"score","score":2.0,"confidence":1.0,"legend":{"0":"none","1":"minor","2":"material"},"probabilities":{"0":0.0,"1":0.0,"2":1.0}}
    })).unwrap(),metrics:Metrics::default(),fallback_reason:None};
    let actions = vec![json!({"id":"native-action"})];
    assert_eq!(
        decision::assessment(&judgment, &actions, 0).state,
        AssessmentState::Uncertain
    );
    if let Answer::Choice { confidence, .. } = judgment.answers.get_mut("alignment").unwrap() {
        *confidence = 0.99;
    }
    assert_eq!(
        decision::assessment(&judgment, &actions, 0).state,
        AssessmentState::MaterialDrift
    );
    assert!(!decision::complete(&judgment, &[]));
}
