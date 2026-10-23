use kah_cli::commit_measure::live::Manifest;
use serde_json::json;
#[test]
fn explicit_replay_stop_is_independent_of_the_cost_estimate() {
    let m:Manifest=serde_json::from_value(json!({"jobs":[],"approved_money_stop_usd":0.15,"option1":{"doubt_line":0.5,"estimate_usd":0.098571127,"authority":"synthetic explicit stop approval"}})).unwrap();
    assert_eq!(m.money_stop().unwrap(), 0.15);
    let old:Manifest=serde_json::from_value(json!({"jobs":[],"option1":{"doubt_line":0.5,"estimate_usd":0.03,"authority":"synthetic legacy multiplier approval"}})).unwrap();
    assert_eq!(old.money_stop().unwrap(), 0.12);
    let invalid: Manifest =
        serde_json::from_value(json!({"jobs":[],"approved_money_stop_usd":-1})).unwrap();
    assert!(invalid.money_stop().is_err());
}
