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
        assert_eq!(Backend::for_domain(domain, None).unwrap(), Backend::Jev);
        assert_eq!(
            Backend::for_domain(domain, Some("jev")).unwrap(),
            Backend::Jev
        );
    }
    let mut judgment=Judgment{backend:"jev".into(),model:"test".into(),answers:serde_json::from_value(json!({
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
    judgment.backend = "luna".into();
    assert_eq!(
        decision::assessment(&judgment, &actions, 2).state,
        AssessmentState::Uncertain
    );
    judgment.answers = serde_json::from_value(json!({
        "outcome_met.one":{"type":"noul","noul":1.0},
        "completion":{"type":"choice","choice":"complete","probabilities":{"complete":1.0},"confidence":1.0}
    })).unwrap();
    assert!(!decision::complete(
        &judgment,
        &[("one".into(), "required outcome".into())]
    ));
}

#[test]
fn score_consistency_tolerance_is_not_defeated_by_float_representation() {
    let criteria = vec!["none".into(), "some".into()];
    let questions = Questions::from([(
        "distance".into(),
        Question::Score {
            instructions: "Rate deviation".into(),
            criteria: criteria.clone(),
        },
    )]);
    let answers = BTreeMap::from([(
        "distance".into(),
        Answer::Score {
            score: 0.07,
            confidence: 0.9,
            legend: BTreeMap::from([
                ("0".into(), criteria[0].clone()),
                ("1".into(), criteria[1].clone()),
            ]),
            probabilities: BTreeMap::from([("0".into(), 0.95), ("1".into(), 0.05)]),
        },
    )]);
    assert!(judge::validate_response(&questions, &answers).is_ok());
}

#[test]
fn completion_requires_every_jev_requirement_to_be_confidently_satisfied() {
    use judge::{Judgment, Metrics, decision};
    let outcomes = vec![("one".into(), "Required behavior".into())];
    let mut judgment = Judgment {backend:"jev".into(), model:"fixture".into(), metrics:Metrics::default(), fallback_reason:None, answers:serde_json::from_value(json!({
        "completion":{"type":"choice","choice":"complete","confidence":0.99,"probabilities":{"complete":0.99,"incomplete":0.0,"unclear":0.01}},
        "outcome_met.one":{"type":"choice","choice":"satisfied","confidence":0.95,"probabilities":{"satisfied":0.96,"violated":0.0,"unclear":0.04}}
    })).unwrap()};
    judgment.answers.remove("completion");
    assert!(decision::complete(&judgment, &outcomes));
    if let Answer::Choice { confidence, .. } = judgment.answers.get_mut("outcome_met.one").unwrap()
    {
        *confidence = 0.8;
    }
    assert!(!decision::complete(&judgment, &outcomes));
    judgment.answers.insert("outcome_met.one".into(), serde_json::from_value(json!({"type":"choice","choice":"unclear","confidence":1.0,"probabilities":{"unclear":1.0,"satisfied":0.0,"violated":0.0}})).unwrap());
    assert!(!decision::complete(&judgment, &outcomes));
}
