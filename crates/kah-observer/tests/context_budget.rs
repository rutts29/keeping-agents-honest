use kah_observer::{
    context_budget::Calibration,
    judge::{Question, Questions},
};
use serde_json::json;
#[test]
fn measured_usage_plans_preparation_without_changing_or_rejecting_content() {
    let calibration = Calibration {
        input_utf8_bytes: 4000,
        input_tokens: 1000,
    };
    let questions = Questions::from([(
        "q".into(),
        Question::Noul {
            instructions: "Read the whole decision.".into(),
        },
    )]);
    let small = json!({"goal":"whole goal"});
    assert!(!calibration.plan(&small, &questions).unwrap().prepare);
    let large = json!({"goal":"Whole canonical statement. ".repeat(6000)});
    let before = large.clone();
    let plan = calibration.plan(&large, &questions).unwrap();
    assert!(plan.prepare && plan.planning_only);
    assert_eq!(large, before);
    assert!(
        Calibration {
            input_utf8_bytes: 0,
            input_tokens: 0
        }
        .plan(&large, &questions)
        .is_none()
    );
}
