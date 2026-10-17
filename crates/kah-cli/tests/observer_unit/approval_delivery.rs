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

#[test]
fn a_terminal_unconfirmed_checkpoint_is_a_decided_non_pass_for_its_delivery() {
    // 5h-b/5h-f/5r-i stalls: Option1 stored passed:null with terminal:true for
    // the exact delivery; completion kept waiting for a pass that never comes.
    let key = Key {
        instance: "run".into(),
        commit: "c".repeat(40),
        user_sequence: 0,
        action_digest: "digest".into(),
        command: "test".into(),
        online: false,
    };
    let checkpoint = json!({"head":key.commit,"intent_revision":2,"passed":null,"terminal":true,"completion_key":key});
    assert!(unconfirmed_terminal(&checkpoint, &key, 2));
    // Only that exact head and intent revision; never a pass, never another SHA.
    assert!(!unconfirmed_terminal(&checkpoint, &key, 3));
    let mut other = key.clone();
    other.commit = "d".repeat(40);
    assert!(!unconfirmed_terminal(&checkpoint, &other, 2));
    let mut passed = checkpoint.clone();
    passed["passed"] = true.into();
    assert!(!unconfirmed_terminal(&passed, &key, 2));
    let mut pending = checkpoint.clone();
    pending["terminal"] = Value::Null;
    assert!(!unconfirmed_terminal(&pending, &key, 2));
}
