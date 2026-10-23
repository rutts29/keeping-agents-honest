mod completion_support;
use kah_observer::judge;

#[test]
fn false_stop_samples_complete_at_the_provisional_completion_bar() {
    let data = completion_support::false_stop_sample();
    let task = data["task"].as_str().unwrap();
    for receipt in data["receipts"].as_array().unwrap() {
        assert_eq!(receipt["completed"], false);
        let mut passed = 0;
        for value in receipt["judgments"].as_array().unwrap() {
            let judgment: judge::Judgment = serde_json::from_value(value.clone()).unwrap();
            let outcomes = judgment
                .answers
                .keys()
                .map(|key| {
                    (
                        key.strip_prefix("outcome_met.").unwrap().to_owned(),
                        task.to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            passed += usize::from(judge::decision::complete(&judgment, &outcomes));
        }
        // Both channels of each false-stop sample (correct work) now pass.
        assert_eq!(passed, 2);
    }
    assert_eq!(judge::questions::UNATTENDED_COMPLETE_PROBABILITY, 0.85);
    assert_eq!(judge::questions::UNATTENDED_COMPLETE_CONFIDENCE, 0.8);
    assert_eq!(judge::questions::COMPLETE_PROBABILITY, 0.9);
    assert_eq!(judge::questions::COMPLETE_CONFIDENCE, 0.85);
}

#[test]
fn the_synthetic_case_has_both_completion_views() {
    let case = completion_support::synthetic_case();
    assert_eq!(case.name, "synthetic-code-task");
    assert_eq!(case.evidence["tests"]["success"], true);
    kah_cli::completion_inputs::states(&case.intent, &case.evidence).unwrap();
}
