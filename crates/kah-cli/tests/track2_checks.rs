#[path = "track2_support/config_audit.rs"]
mod config_audit;
#[path = "track2_support/dialog.rs"]
mod dialog;
#[path = "track2_support/hook_events.rs"]
mod hook_events;
#[path = "track2_support/proof.rs"]
mod proof;
#[path = "track2_support/usage.rs"]
mod usage;
use serde_json::json;

fn checkpoint(id: &str) -> serde_json::Value {
    json!({"request":id,"purpose":"checkpoint","observer_session":"same",
        "pane":"w1:p3","native_tokens":7,"usage_gap":null,
        "judgment":{"backend":"jev","metrics":{"calls":1}},
        "report":{"evidence":[{"citations":[{"source":"previous-review",
        "quote":"EARLY_FINDING"}]}]}})
}

#[test]
fn memory_requires_three_distinct_checkpoints_same_session_pane_usage_and_citation() {
    let good = vec![checkpoint("r1"), checkpoint("r2"), checkpoint("r3")];
    assert!(proof::memory(&good, "EARLY_FINDING").is_ok());
    for (index, key, value) in [
        (1, "purpose", json!("drift")),
        (1, "request", json!("r1")),
        (2, "observer_session", json!("different")),
        (2, "pane", json!("w1:p4")),
        (2, "native_tokens", json!(0)),
        (2, "usage_gap", json!("missing")),
        (2, "report", json!({})),
    ] {
        let mut broken = good.clone();
        broken[index][key] = value;
        assert!(proof::memory(&broken, "EARLY_FINDING").is_err(), "{key}");
    }
    assert!(proof::memory(&good[..2], "EARLY_FINDING").is_err());
}

#[test]
fn latency_is_nearest_rank_with_missing_and_clock_errors_rejected() {
    let commits = json!([{"sha":"a","committed_ms":1,"verdict_ms":600001},
        {"sha":"b","committed_ms":1,"verdict_ms":900001},
        {"sha":"c","committed_ms":1,"verdict_ms":1800001}]);
    let result = proof::latency(commits.as_array().unwrap()).unwrap();
    assert_eq!(result["p50_ms"], 900000);
    assert_eq!(result["p90_ms"], 1800000);
    assert_eq!(result["passed"], true);
    for row in [
        json!({"sha":"x","committed_ms":1}),
        json!({"sha":"x","committed_ms":2,"verdict_ms":1}),
    ] {
        assert!(proof::latency(&[row]).is_err());
    }
    assert!(proof::latency(&[]).is_err());
}

#[test]
fn growing_backlog_or_empty_completion_interval_cannot_pass() {
    assert!(proof::drain(4, 3, 2, 0, 3600000).unwrap()["passed"] == true);
    assert!(proof::drain(3, 4, 2, 0, 3600000).unwrap()["passed"] == false);
    assert!(proof::drain(0, 0, 0, 0, 3600000).is_err());
    assert!(proof::drain(0, 0, 1, 5, 5).is_err());
}
#[test]
fn shared_claude_counters_can_change_but_fixture_trust_and_session_entries_fail() {
    let root = std::env::temp_dir().join(format!("track2-config-test-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("config.json");
    std::fs::write(
        &path,
        r#"{"numRuns":1,"private":"PRIVATE_VALUE_MUST_NOT_LEAK"}"#,
    )
    .unwrap();
    let before = config_audit::snapshot(&path, &root);
    assert!(!before.to_string().contains("PRIVATE_VALUE_MUST_NOT_LEAK"));
    std::fs::write(
        &path,
        r#"{"numRuns":2,"private":"PRIVATE_VALUE_MUST_NOT_LEAK"}"#,
    )
    .unwrap();
    let changed = config_audit::snapshot(&path, &root);
    assert_eq!(config_audit::compare(&before, &changed)["passed"], true);
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"projects":{root.to_str().unwrap():{"hasTrustDialogAccepted":true}}}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        config_audit::compare(&before, &config_audit::snapshot(&path, &root))["passed"],
        false
    );
    std::fs::write(root.join("sessions.json"), r#"["fixture-native-id"]"#).unwrap();
    std::fs::write(&path, r#"{"lastSessionId":"fixture-native-id"}"#).unwrap();
    assert_eq!(
        config_audit::compare(&before, &config_audit::snapshot(&path, &root))["passed"],
        false
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn folder_consent_accepts_only_codex_exact_worktree_and_selected_trust_choice() {
    let root = std::path::Path::new("/private/fixture/worktree");
    let screen =
        "Folder access\n/private/fixture/worktree\nTrust this folder?\n› 1. Trust and continue";
    assert_eq!(dialog::folder_dialog("codex", screen, root), Ok(true));
    for (provider, frame) in [
        ("claude", screen),
        ("codex", "Approve this command?"),
        ("codex", "Login required"),
        ("codex", "Sandbox permissions"),
        ("codex", "Update available"),
        (
            "codex",
            "Folder access\n/private/fixture/worktree-evil\nTrust this folder?\n› 1. Trust and continue",
        ),
        (
            "codex",
            "Folder access\n/private/fixture/worktree\nTrust this folder?\n1. Trust and continue\n› 2. Quit",
        ),
    ] {
        assert!(dialog::folder_dialog(provider, frame, root).is_err());
    }
}
#[test]
fn perturbation_files_must_prove_conditions_not_just_exist() {
    let good = json!({"observer-restart":{"session_before":"same","session_after":"same","pid_before":1,"pid_after":2},
        "binary-swap":{"mid_review":true,"before_sha256":"old","after_sha256":"new"},
        "post-ack":{"actual_native_ack":true,"redelivery_unchanged":true,"start_ms":0,"end_ms":20000,
            "progress":{"stalled":false,"sample":{"controller_active":true}}},
        "host-restart":{"fixture_owned":true,"observer_session":"same","host_pid_before":3,"host_pid_after":4},
        "concurrent":{"statuses":[true,true,true]}});
    assert!(proof::perturbations(&good).is_ok());
    for name in [
        "observer-restart",
        "binary-swap",
        "post-ack",
        "host-restart",
        "concurrent",
    ] {
        let mut missing = good.clone();
        missing[name] = json!({});
        assert!(proof::perturbations(&missing).is_err(), "{name}");
    }
    let mut changed = good.clone();
    changed["binary-swap"]["mid_review"] = false.into();
    assert!(proof::perturbations(&changed).is_err());
    changed = good.clone();
    changed["post-ack"]["redelivery_unchanged"] = false.into();
    assert!(proof::perturbations(&changed).is_err());
}
#[test]
fn usage_skips_null_codex_counters_and_keeps_max_charge_of_claude_stream_revisions() {
    let codex = concat!(
        "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":40,\"output_tokens\":10}}}}\n",
        "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":null}}\n",
        "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":150,\"cached_input_tokens\":70,\"output_tokens\":20}}}}\n"
    );
    let result = usage::parse(std::io::Cursor::new(codex), "codex");
    assert_eq!(result["input"], 150);
    assert_eq!(result["cached_input"], 70);
    let claude = concat!(
        "{\"type\":\"assistant\",\"message\":{\"id\":\"one\",\"usage\":{\"input_tokens\":3,\"cache_read_input_tokens\":9,\"cache_creation_input_tokens\":4,\"output_tokens\":7}}}\n",
        "{\"type\":\"assistant\",\"message\":{\"id\":\"one\",\"usage\":{\"input_tokens\":0,\"output_tokens\":0}}}\n"
    );
    let result = usage::parse(std::io::Cursor::new(claude), "claude");
    assert_eq!(
        result,
        json!({"input":3,"cached_input":9,"cache_creation":4,"output":7})
    );
}

#[test]
fn current_long_session_hooks_require_intake_and_stop_without_the_retired_tool_lane() {
    let current = ["SessionStart", "UserPromptSubmit", "Stop"].map(|event| json!({"event":event}));
    assert!(hook_events::complete(&current));
    assert!(!hook_events::complete(&current[..2]));
    assert!(!hook_events::complete(&[json!({"event":"PostToolUse"})]));
}
