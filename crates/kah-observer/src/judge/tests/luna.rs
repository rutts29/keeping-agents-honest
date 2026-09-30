use super::*;

#[test]
fn zero_mass_distribution_gets_one_repair_and_both_calls_are_counted() {
    let questions = Questions::from([(
        "kind".into(),
        Question::Choice {
            instructions: "Choose none if no new steering exists.".into(),
            criteria: BTreeMap::from([
                ("none".into(), "No steering".into()),
                ("change".into(), "Steering".into()),
            ]),
        },
    )]);
    let mut calls = 0;
    let (answers, metrics) = validated_call(&questions, "fixture", |prompt| {
        calls += 1;
        if calls == 2 { assert!(prompt.contains("JUDGE_DISTRIBUTION")); }
        Ok((json!({"kind":{"probabilities":{"none":if calls == 1 {0.0} else {1.0},"change":0.0},"confidence":0.99}}), Metrics { calls:1, input_tokens:10, output_tokens:2, elapsed_ms:3, ..Default::default() }))
    }).unwrap();
    assert_eq!(calls, 2);
    assert_eq!(metrics.calls, 2);
    assert_eq!(metrics.input_tokens, 20);
    assert_eq!(metrics.elapsed_ms, 6);
    validate_response(&questions, &answers).unwrap();
    let mut failed_calls = 0;
    assert!(
        validated_call(&questions, "fixture", |_| {
            failed_calls += 1;
            Ok((
                json!({"kind":{"probabilities":{"none":0.0,"change":0.0},"confidence":0.99}}),
                Metrics::default(),
            ))
        })
        .is_err()
    );
    assert_eq!(failed_calls, 2, "repair is bounded");
}
