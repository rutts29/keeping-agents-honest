use kah_cli::observer_review::state;
use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[test]
fn removing_transient_files_during_reads_is_absence_not_an_observer_gap() {
    let root = std::env::temp_dir().join(format!("kah-review-read-race-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let expected = json!({"whole":"Verbatim complete item.\n".repeat(1000)});
    let finished = Arc::new(AtomicBool::new(false));
    let writer_root = root.clone();
    let writer_expected = expected.clone();
    let writer_finished = finished.clone();
    let writer = std::thread::spawn(move || {
        for _ in 0..2000 {
            kah_cli::unattended_report::save(&writer_root, "ephemeral.json", &writer_expected)
                .unwrap();
            fs::remove_file(writer_root.join("ephemeral.json")).unwrap();
        }
        writer_finished.store(true, Ordering::Release);
    });
    while !finished.load(Ordering::Acquire) {
        if let Some(value) = state::read::<Value>(&root.join("ephemeral.json")).unwrap() {
            assert_eq!(value, expected);
        }
    }
    writer.join().unwrap();
    assert!(
        state::read::<Value>(&root.join("ephemeral.json"))
            .unwrap()
            .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn descriptor_reads_still_reject_symlinks_without_opening_the_target() {
    let root = std::env::temp_dir().join(format!("kah-review-read-link-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    fs::write(root.join("target"), "{}").unwrap();
    std::os::unix::fs::symlink(root.join("target"), root.join("link")).unwrap();
    assert!(
        state::read::<Value>(&root.join("link"))
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_STATE_PRIVATE")
    );
    fs::remove_dir_all(root).unwrap();
}
