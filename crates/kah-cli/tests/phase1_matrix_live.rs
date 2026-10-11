mod phase1_matrix_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[test]
#[ignore = "ONE full native persistent matrix session; make gate only; no Jev"]
fn persistent_native_matrices() {
    phase1_matrix_support::launch();
}
#[test]
#[ignore = "decisive Step1 clean native atomization; make gate only; zero Jev"]
fn atomize_current_intent() {
    phase1_matrix_support::launch();
}
#[test]
#[ignore = "reviewer-authorized real Jev confirmation in SAME native session; make gate only"]
fn confirm_real_jev() {
    phase1_matrix_support::confirmation_replay();
}
#[test]
fn matrix_entrypoint() {
    if std::env::var_os("KAH_PHASE1_ROOT").is_some() {
        phase1_matrix_support::entry();
    }
}
#[test]
fn native_resume_usage_selects_bound_id_over_empty_startup_stub() {
    // Observed 0.162.1 resume failure: three startup rows in another file;
    // original thread actually answered. Official rust-v0.162.1 CLI main.rs
    // ResumeCommand binds SESSION_ID. Fixture projects only required metadata;
    // the replay input preserves complete source records.
    let root = std::env::temp_dir().join(format!("phase1-usage-{}", kah_cli::relay_store::nonce()));
    let sessions = root.join(".codex/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join("stub.jsonl"),
        include_str!("../../../docs/gates/evidence/observer-phase1/resume-records/stub.jsonl"),
    )
    .unwrap();
    let resumed =
        include_str!("../../../docs/gates/evidence/observer-phase1/resume-records/resumed.jsonl");
    std::fs::write(sessions.join("resumed.jsonl"), resumed).unwrap();
    let meta: serde_json::Value = serde_json::from_str(resumed.lines().next().unwrap()).unwrap();
    let project = std::path::Path::new(meta["payload"]["cwd"].as_str().unwrap());
    let id = meta["payload"]["id"].as_str().unwrap();
    let result = phase1_matrix_support::usage::snapshot_for(&root, project, Some(id));
    assert_eq!(result["session"], id);
    assert_eq!(result["model"], "gpt-6.1-sol");
    assert_eq!(result["effort"], "low");
    assert!(result["usage"]["input_tokens"].as_u64().unwrap() > 0);
}
