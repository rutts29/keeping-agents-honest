use kah_cli::completion_review::{Key, verified};
use kah_run::verification::Receipt;
use std::{cell::Cell, fs};

fn key() -> Key {
    Key {
        instance: "controller-1".into(),
        commit: "commit-a".into(),
        user_sequence: 1,
        action_digest: "actions-a".into(),
        command: "test integration".into(),
        online: false,
    }
}
fn receipt(key: &Key) -> Receipt {
    Receipt {
        commit: key.commit.clone(),
        command: key.command.clone(),
        success: true,
        exit_code: Some(0),
        elapsed_ms: 10,
        output: "integration passed".into(),
        output_truncated: false,
    }
}

#[test]
fn waiting_for_review_reuses_tests_but_changed_completion_inputs_reverify() {
    let root = std::env::temp_dir().join(format!("kah-completion-cache-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let calls = Cell::new(0);
    let original = key();
    let check = |key: &Key| {
        verified(&root, key, || {
            calls.set(calls.get() + 1);
            Ok((receipt(key), false))
        })
        .unwrap()
    };
    let first = check(&original);
    assert_eq!(check(&original).id().unwrap(), first.id().unwrap());
    assert_eq!(calls.get(), 1);
    let mut variants = vec![original.clone(); 6];
    variants[0].instance = "resumed-controller".into();
    variants[1].commit = "commit-b".into();
    variants[2].user_sequence = 2;
    variants[3].action_digest = "new-native-actions".into();
    variants[4].command = "test other-suite".into();
    variants[5].online = true;
    for variant in variants {
        assert_ne!(check(&variant).id().unwrap(), first.id().unwrap());
    }
    assert_eq!(calls.get(), 7);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_verification_receipt_cannot_be_cached_under_another_commit() {
    let root =
        std::env::temp_dir().join(format!("kah-completion-cache-wrong-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let key = key();
    let mut wrong = receipt(&key);
    wrong.commit = "another-commit".into();
    let error = verified(&root, &key, || Ok((wrong, false))).err().unwrap();
    assert!(error.to_string().starts_with("RUN_VERIFICATION_IDENTITY"));
    assert!(!root.join("completion-verification.json").exists());
    fs::remove_dir_all(root).unwrap();
}
