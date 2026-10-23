//! Restricted routing negatives; the global-daemon probe was retired.
#[path = "journeys/fixture.rs"]
mod fixture;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
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
