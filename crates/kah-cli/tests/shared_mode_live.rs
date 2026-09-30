//! Read-only global-config probe plus disposable Mine thread metadata; no tools run.
#[path = "journeys/fixture.rs"]
mod fixture;
use kah_codex::control::Control;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command, time::Duration};
#[test]
#[ignore = "opt-in shared-daemon policy and hook probe"]
fn shared_daemon_policy_probe() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = (
        fs::read(home.join(".codex/config.toml")).unwrap(),
        fs::read(home.join(".codex/hooks.json")).unwrap(),
    );
    let (launch, mut mine) = fixture::prepare(&home, "mine", false);
    // 0.159.2 persists trust for new writable cwd values, even for ephemeral
    // threads. Use this already-trusted checkout, without executing tools.
    let cwd = std::env::current_dir().unwrap();
    let mut global = Control::connect_daemon(
        &home.join(".codex/app-server-control/app-server-control.sock"),
        Duration::from_secs(20),
    )
    .unwrap();
    let missing=global.request("thread/start",json!({"cwd":cwd,"permissions":"kah-mine-workspace","approvalPolicy":"on-request","approvalsReviewer":"auto_review","ephemeral":true}));
    assert!(
        missing.is_err(),
        "kah profile unexpectedly installed in global daemon"
    );
    let started=global.request("thread/start",json!({"cwd":cwd,"permissions":":workspace","approvalPolicy":"on-request","approvalsReviewer":"auto_review","ephemeral":true})).unwrap();
    assert_eq!(started["approvalPolicy"], "on-request");
    let id = started["thread"]["id"].as_str().unwrap();
    mine.sessions.push(id.into());
    let loaded = global.request("thread/loaded/list", json!({})).unwrap();
    assert!(loaded["data"].as_array().unwrap().iter().any(|v| v == id));
    let hooks = global.request("hooks/list", json!({"cwds":[cwd]})).unwrap();
    let entries = hooks["data"][0]["hooks"].as_array().unwrap();
    assert!(!entries.iter().any(|h| {
        h["command"]
            .as_str()
            .is_some_and(|c| c.contains("kah' __hook") || c.contains("kah __hook"))
    }));
    let model_args = launch
        .command(false)
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(!model_args.iter().any(|s| s == "-c" || s == "--config"));
    let overlay_version = Command::new(home.join(".local/bin/codex"))
        .args(["app-server", "daemon", "version"])
        .env("CODEX_HOME", &mine.overlay)
        .output()
        .unwrap();
    let version = json!({"exit":overlay_version.status.code(),"separateOverlayEndpoint":!overlay_version.status.success()});
    let _ = global.request("thread/unsubscribe", json!({"threadId":id}));
    let negatives = remote_rejected(&home);
    assert!(
        before
            == (
                fs::read(home.join(".codex/config.toml")).unwrap(),
                fs::read(home.join(".codex/hooks.json")).unwrap()
            ),
        "Global config/hooks changed during shared probe; inspect privately"
    );
    let path = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(path.join("shared-mode-probe.json"),serde_json::to_string_pretty(&json!({"metadataProbeCompleted":true,"mineSharedWithEhObserverProven":false,"globalConfigAndHooksUnchanged":true,"mineBuiltInWorkspaceThreadAccepted":true,"globalThreadLoaded":true,"kahProfileRejected":missing.unwrap_err().to_string(),"globalHooksHaveEhObserver":false,"threadPolicy":{"permissions":started["permissions"],"sandbox":started["sandbox"],"approvalPolicy":started["approvalPolicy"],"approvalsReviewer":started["approvalsReviewer"]},"overlayDaemonVersion":version,"tuiArgs":model_args,"restrictedRemoteNegative":negatives,"remoteDeviceUI":"not directly observed; no device reconfiguration or pairing attempted","newDaemonStarted":false})).unwrap()).unwrap();
}
fn remote_rejected(home: &std::path::Path) -> Vec<Value> {
    ["research", "untrusted"]
        .into_iter()
        .map(|domain| {
            let (_, f) = fixture::prepare(home, domain, false);
            let output = Command::new(env!("CARGO_BIN_EXE_kah"))
                .args([
                    domain,
                    f.project.file_name().unwrap().to_str().unwrap(),
                    "--",
                    "--remote",
                    "unix://",
                ])
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("NATIVE_REMOTE_POLICY"));
            json!({"domain":domain,"refusedBeforeGlobalConnection":true})
        })
        .collect()
}

#[test]
#[ignore = "installed launcher rejects Research/Untrusted global-daemon routing before launch"]
fn restricted_domains_refuse_shared_remote() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let report = remote_rejected(&home);
    let directory = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        directory.join("restricted-shared-negative.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"attempts":report})).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "passive daemon version/socket lookup for isolated homes; never starts a daemon"]
fn overlay_homes_have_separate_daemon_endpoints() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let mut results = vec![];
    for domain in ["mine", "research"] {
        let (_, f) = fixture::prepare(&home, domain, false);
        let output = Command::new(home.join(".local/bin/codex"))
            .args(["app-server", "daemon", "version"])
            .env("CODEX_HOME", &f.overlay)
            .output()
            .unwrap();
        let socket = f.overlay.join("app-server-control/app-server-control.sock");
        let version = if output.status.success() {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_ne!(value["status"], "running");
            assert!(
                value["socketPath"]
                    .as_str()
                    .is_some_and(|s| std::path::Path::new(s).starts_with(&f.overlay))
            );
            value
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains(socket.to_str().unwrap())
                    && error.contains("No such file or directory")
            );
            json!({"status":"no_listening_daemon","socketPath":socket,"versionQueryExit":output.status.code(),"reason":"ENOENT at overlay endpoint"})
        };
        results.push(json!({"domain":domain,"version":version,"newDaemonStarted":false}));
    }
    let directory = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        directory.join("overlay-daemon-endpoints.json"),
        serde_json::to_vec_pretty(&json!({"passed":true,"results":results})).unwrap(),
    )
    .unwrap();
}
