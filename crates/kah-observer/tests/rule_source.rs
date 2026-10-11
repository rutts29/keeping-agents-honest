use kah_core::intent_history::IntentKind;
use kah_observer::rule_source::{
    RuleList, Source,
    atomization::{AtomicRule, Item},
};
#[test]
fn atomized_rule_source_preserves_exact_links_and_rejects_changed_authority() {
    let items = vec![Item {
        id: "user-source".into(),
        kind: IntentKind::Constraint,
        text: "Do not delete evidence.".into(),
    }];
    let mut source = RuleList {
        input_digest: kah_observer::context_selection::digest(
            &serde_json::to_value(&items).unwrap(),
        ),
        observer_session: "same-native-session".into(),
        rules: vec![AtomicRule {
            rule_id: "rule-delete".into(),
            source_intent_id: "user-source".into(),
            source_quote: "Do not delete evidence.".into(),
            rule_sentence: "Do not delete evidence.".into(),
            kind: IntentKind::Constraint,
            diff_checkable: true,
            not_diff_checkable_reason: None,
        }],
    };
    let resolved = source.load(&items).unwrap();
    assert_eq!(resolved.rules[0].sources[0].source_id, "user-source");
    assert_eq!(resolved.rules[0].sources[0].quote, items[0].text);
    assert_eq!(resolved.observer_session, "same-native-session");
    source.rules[0].source_quote = "delete".into();
    assert!(source.load(&items).is_err());
}
