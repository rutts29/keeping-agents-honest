use super::*;
#[test]
fn only_the_exact_answer_can_extend_an_approved_delivery_sequence() {
    let root = std::env::temp_dir().join(format!(
        "kah-answer-delivery-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let mut control = kah_core::unattended::RunControl::new("task", Default::default()).unwrap();
    control.start().unwrap();
    control.steer("Accept the named risk.").unwrap();
    let record = kah_run::store::Record {
        project: root.clone(),
        worktree: root.clone(),
        domain: "research".into(),
        model: None,
        effort: None,
        control,
        native: None,
    };
    crate::unattended_report::save(&root, "state.json", &serde_json::to_value(&record).unwrap())
        .unwrap();
    let original = Key {
        instance: "old".into(),
        commit: "a".repeat(40),
        user_sequence: 0,
        action_digest: "old".into(),
        command: "test".into(),
        online: false,
    };
    let checkpoint = json!({"state":"checkpoint_user_approved","head":original.commit,"completion_key":original,"user_answer":{"verbatim":"Accept the named risk."}});
    let mut current = original.clone();
    current.user_sequence = 1;
    assert!(approval_delivery(&root, &checkpoint, &current).unwrap());
    let mut other = checkpoint.clone();
    other["user_answer"]["verbatim"] = "Unrelated instruction".into();
    assert!(!approval_delivery(&root, &other, &current).unwrap());
    current.commit = "b".repeat(40);
    assert!(!approval_delivery(&root, &checkpoint, &current).unwrap());
    current.commit = original.commit;
    current.online = true;
    assert!(!approval_delivery(&root, &checkpoint, &current).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
