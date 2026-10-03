use kah_core::{
    intent_history::SourceRole,
    observation::{ContextInjection, InjectionKind},
    session_bundle::BundleRecord,
};

#[test]
fn large_source_records_and_context_injections_keep_the_complete_final_instruction() {
    let text = format!(
        "{}\nFinal user instruction: keep notes/ out of commits.\n",
        "Whole original paragraph 完整.\n".repeat(40000)
    );
    let record = BundleRecord::new("user", SourceRole::User, &text).unwrap();
    assert_eq!(record.content(), text);
    let injection = ContextInjection::new(1, InjectionKind::DriftReminder, &text).unwrap();
    let injected = injection.as_record("context").unwrap();
    assert_eq!(injected.content(), text);
}
