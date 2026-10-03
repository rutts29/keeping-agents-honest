use kah_cli::observer_review::consume::read_report;
use std::{fs, os::unix::fs::symlink};
#[test]
fn report_cannot_read_host_symlink_or_unbounded_file() {
    let root = std::env::temp_dir().join(format!("kah-review-report-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("report.json");
    fs::write(&file, b"{}").unwrap();
    assert_eq!(read_report(&file).unwrap(), b"{}");
    let link = root.join("link.json");
    symlink(&file, &link).unwrap();
    assert!(read_report(&link).is_err());
    assert!(read_report(&root).is_err());
    fs::write(&file, vec![b' '; 65537]).unwrap();
    assert!(read_report(&file).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn first_report_creates_a_private_durable_archive() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("kah-review-archive-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let value = serde_json::json!({"request":"review-first","decision":"jev"});
    kah_cli::observer_review::consume::archive(&root, "review-first", &value).unwrap();
    assert_eq!(
        fs::metadata(root.join("results"))
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
    let path = root.join("results/review-first.json");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&path).unwrap()).unwrap(),
        value
    );
    assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
    assert!(kah_cli::observer_review::consume::archive(&root, "../escape", &value).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn native_subdirectories_stay_bound_but_symlink_escapes_do_not() {
    let root = std::env::temp_dir().join(format!("kah-review-cwd-{}", std::process::id()));
    fs::create_dir_all(root.join("observer/inputs/request")).unwrap();
    fs::create_dir(root.join("outside")).unwrap();
    let workspace = root.join("observer");
    symlink(root.join("outside"), workspace.join("escape")).unwrap();
    let contains = |path: &std::path::Path| {
        kah_cli::observer_review::control::contains_workspace(&workspace, path.to_str().unwrap())
    };
    assert!(contains(&workspace));
    assert!(contains(&workspace.join("inputs/request")));
    assert!(!contains(&workspace.join("escape")));
    assert!(!contains(&root.join("outside")));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn claude_usage_uses_native_path_and_missing_history_is_not_zero_usage() {
    let home = std::env::temp_dir().join(format!("kah-review-usage-{}", std::process::id()));
    let directory = home.join(".claude/projects/-home-user--cache-observer");
    fs::create_dir_all(&directory).unwrap();
    let session = "00000000-0000-4000-8000-000000000001";
    let path = directory.join(format!("{session}.jsonl"));
    fs::write(&path,serde_json::json!({"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":7,"output_tokens":3,"cache_read_input_tokens":5}}}).to_string()+"\n").unwrap();
    assert_eq!(
        kah_cli::observer_review::consume::claude_tokens(&home, &path, session).unwrap(),
        15
    );
    assert!(
        kah_cli::observer_review::consume::claude_tokens(&home, &path, "another-session").is_err()
    );
    fs::remove_file(&path).unwrap();
    assert!(kah_cli::observer_review::consume::claude_tokens(&home, &path, session).is_err());
    fs::remove_dir_all(home).unwrap();
}
#[test]
fn process_watcher_waits_for_exit_using_a_regular_pidfile() {
    let root = std::env::temp_dir().join(format!("kah-review-pid-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut child = std::process::Command::new("/bin/sleep")
        .arg("0.2")
        .spawn()
        .unwrap();
    let stamp = kah_observer::session::process_stamp(child.id()).unwrap();
    kah_cli::observer_review::lifecycle::wait_exit(&root, child.id(), &stamp).unwrap();
    assert!(
        child.try_wait().unwrap().is_some(),
        "watcher returned before child exited"
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn only_a_native_user_receipt_confirms_delivery_and_enter_is_composer_scoped() {
    use kah_cli::observer_review::delivery::{composer_contains, receipt, safe_keyboard};
    use std::io::Cursor;
    let marker = "kah review request review-123 ";
    let message = serde_json::json!({"type":"user","sessionId":"s","message":{"content":format!("{marker}(Drift). Read sources.")}});
    assert!(receipt(Cursor::new(message.to_string()), "claude", "s", marker).unwrap());
    let mut fake = message.clone();
    fake["type"] = "assistant".into();
    assert!(!receipt(Cursor::new(fake.to_string()), "claude", "s", marker).unwrap());
    fake = message;
    fake["message"]["content"] = serde_json::json!([{"type":"tool_result","content":marker}]);
    assert!(!receipt(Cursor::new(fake.to_string()), "claude", "s", marker).unwrap());
    let native = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":format!("{marker}(Drift)")} ]}});
    assert!(receipt(Cursor::new(native.to_string()), "codex", "s", marker).unwrap());
    assert!(composer_contains(
        &format!("history\n❯ {marker}(Drift)\nfooter"),
        marker,
        "claude"
    ));
    assert!(!composer_contains(
        &format!("❯ {marker}\nold answer\n❯ "),
        marker,
        "claude"
    ));
    for word in ["billing", "CREDIT", "purchase", "upgrade", "payment"] {
        assert!(safe_keyboard(&format!("❯ {marker} {word}")).is_err());
    }
}

#[test]
fn claude_bracketed_paste_receipt_is_recognized_without_matching_quoted_history() {
    use kah_cli::observer_review::delivery::receipt;
    use std::io::Cursor;
    let marker = "kah review request review-123 ";
    for (text, expected) in [
        (
            "\n\n<pasted_content id=\"9ac2\">\nkah review request review-123 (Drift). Read files.\n</pasted_content id=\"9ac2\">\n",
            true,
        ),
        (
            "<pasted_content id=\"9ac2\">\nkah review request review-123 (Drift).\n</pasted_content id=\"abcd\">",
            false,
        ),
        (
            "\n\n<pasted_content id=\"abbd\">\nkah review request review-123 (Drift). Read files.\n</pasted_content>",
            false,
        ),
        (
            "Earlier message: kah review request review-123 (Drift).",
            false,
        ),
        (
            "<pasted_content id=\"abbd\">\nA quote: kah review request review-123 (Drift).\n</pasted_content>",
            false,
        ),
    ] {
        let row = serde_json::json!({"type":"user","sessionId":"s","message":{"role":"user","content":text}});
        assert_eq!(
            receipt(Cursor::new(row.to_string()), "claude", "s", marker).unwrap(),
            expected
        );
    }
}
