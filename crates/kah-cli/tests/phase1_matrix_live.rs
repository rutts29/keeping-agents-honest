//! Live native journeys on a generated corpus (phase1_matrix_support/corpus.rs).
//! Opt in with `KAH_LIVE_CONTROLLER=1 KAH_PHASE1_EVIDENCE=<new directory>`, then
//! `cargo test -p kah-cli --test phase1_matrix_live -- --ignored --exact <test>`.
//! They need native Codex and Herdr. `atomize_current_intent` needs
//! `KAH_PHASE1_REPLAY_STAGE=atomize`, or `atomize-refine` for a draft and its
//! refinement in one native session.
//!
//! Further env-gated stages of `persistent_native_matrices` take operator
//! inputs (none ship with the tests):
//! - `KAH_OPTION1_CHECKPOINT_INPUT`: a checkpoint commit_check Input JSON, with
//!   `KAH_OPTION1_FROZEN_RULES` naming its rule file; runs one full checkpoint.
//! - `KAH_PHASE1_SELECTION_JOBS`: a JSON array of `{id,input}` preparation jobs
//!   for whole-item selection in one session.
//! - `KAH_PHASE1_RESUME_SESSION`: the native session id the stages must keep.
//! - `KAH_PHASE1_REPLAY_STAGE=option1-e2e` (with `KAH_OPTION1_LARGE_SOURCE`, a
//!   large JS data file, and `KAH_OPTION1_REAL_HOME`, the home holding the Jev
//!   key) or `current-rules-e2e`: the full worker and observer route.
mod phase1_matrix_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[test]
#[ignore = "one live native persistent matrix session; opt in as described above; no Jev"]
fn persistent_native_matrices() {
    phase1_matrix_support::launch();
}
#[test]
#[ignore = "live clean native atomization; opt in as described above; zero Jev"]
fn atomize_current_intent() {
    phase1_matrix_support::launch();
}
#[test]
#[ignore = "live confirmation of labelled flags in one native session; opt in as described above"]
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
    // A resume can leave a startup stub for the same project in another file;
    // the bound resumed session (codex resume SESSION_ID) has the usage.
    use serde_json::json;
    let root = std::env::temp_dir().join(format!("phase1-usage-{}", kah_cli::relay_store::nonce()));
    let sessions = root.join(".codex/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let project = root.join("project");
    let id = "00000000-0000-7000-8000-00000000a001";
    let meta = |id: &str| json!({"type":"session_meta","payload":{"id":id,"cwd":project}});
    std::fs::write(
        sessions.join("stub.jsonl"),
        format!("{}\n", meta("00000000-0000-7000-8000-00000000a002")),
    )
    .unwrap();
    let usage = json!({"input_tokens":5200,"cached_input_tokens":4800,"output_tokens":90,"total_tokens":5290});
    let rows = [
        meta(id),
        json!({"type":"turn_context","payload":{"model":"gpt-6.1-sol","effort":"low"}}),
        json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":usage}}}),
    ];
    let resumed = rows.iter().map(|r| format!("{r}\n")).collect::<String>();
    std::fs::write(sessions.join("resumed.jsonl"), resumed).unwrap();
    let result = phase1_matrix_support::usage::snapshot_for(&root, &project, Some(id));
    assert_eq!(result["session"], id);
    assert_eq!(result["model"], "gpt-6.1-sol");
    assert_eq!(result["effort"], "low");
    assert!(result["usage"]["input_tokens"].as_u64().unwrap() > 0);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn generated_corpus_has_measured_cases_atomization_input_and_bound_flags() {
    let root =
        std::env::temp_dir().join(format!("phase1-corpus-{}", kah_cli::relay_store::nonce()));
    let corpus = phase1_matrix_support::corpus::generate(&root);
    let read = |path: std::path::PathBuf| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let report = read(corpus.join("report.json"));
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let directory = std::path::PathBuf::from(case["directory"].as_str().unwrap());
        let bundle: kah_cli::commit_measure::Bundle =
            serde_json::from_value(read(directory.join("all_changed_files/input.json"))).unwrap();
        assert_eq!(bundle.input.revision, case["head"].as_str().unwrap());
        assert_eq!(bundle.input.hunks.len(), 1);
        assert!(
            directory
                .join("all_changed_files/matrix-input.json")
                .is_file()
        );
    }
    let items: Vec<kah_cli::commit_measure::atomization::Item> =
        serde_json::from_value(read(corpus.join("intent-only.json"))).unwrap();
    assert_eq!(items.len(), 2);
    let flags = read(phase1_matrix_support::corpus::flags(&corpus).join("complete.json"));
    for record in flags["records"].as_array().unwrap() {
        let job = read(record["job"]["path"].as_str().unwrap().into());
        let request = std::fs::read(job["request"].as_str().unwrap()).unwrap();
        assert_eq!(
            job["request_digest"],
            kah_observer::commit_check::digest(&request)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
