#[allow(dead_code)]
mod claude_support;
#[allow(dead_code)]
#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/herdr_probe.rs"]
mod herdr_probe;
#[allow(dead_code, unused_imports)]
mod native_controller_support;
#[path = "../src/privacy.rs"]
mod privacy;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
mod relay_support;
#[allow(dead_code)]
#[path = "track2_support/verify.rs"]
mod track2_support;
#[path = "terminal_support/version.rs"]
mod version;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command, time::Duration};

#[test]
#[ignore = "early verify-first journey with private kah/HOME/Herdr; make gate only"]
fn early_verification_first_private() {
    track2_support::launch("verify");
}
#[test]
fn early_verify_entrypoint() {
    if std::env::var("KAH_TRACK2_ENTRYPOINT").as_deref() == Ok("verify") {
        let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
        let run = std::panic::catch_unwind(|| relay_support::run(&root));
        let mut result=run.unwrap_or_else(|_|json!({"passed":false,"reason":"TRACK2_VERIFY_PANIC","trace":root.join("helper.log")}));
        result["early_run"] = true.into();
        result["final_acceptance"] = false.into();
        fs::write(
            root.join("result.json"),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "two-way native relay and both native worker dispatches in a private Herdr server"]
fn relay_and_dispatch() {
    let versions = [
        version::Version::capture("codex"),
        version::Version::capture("claude"),
    ];
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join("native-versions-before.json"),
        serde_json::to_vec_pretty(&json!({"codex":versions[0],"claude":versions[1]})).unwrap(),
    )
    .unwrap();
    let probe = herdr_probe::setup();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        pty::quote(probe.root.join("config.toml").to_str().unwrap()),
        pty::quote(&probe.name)
    );
    let mut terminal = pty::Terminal::start(&command, &probe.root.join("terminal.log"));
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.submit(&format!(
        "KAH_RELAY_PROBE={} {} --exact relay_probe_entrypoint --nocapture > {} 2>&1",
        pty::quote(probe.root.to_str().unwrap()),
        pty::quote(std::env::current_exe().unwrap().to_str().unwrap()),
        pty::quote(probe.root.join("helper.log").to_str().unwrap())
    ));
    let (evidence, reviewer_notified) = wait_evidence(&probe.root, &mut terminal);
    if evidence["passed"] != true {
        let path = PathBuf::from(format!("/tmp/kah-relay-failure-{}.log", std::process::id()));
        fs::copy(probe.root.join("helper.log"), &path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        if probe.root.join("submit.json").exists() {
            let trace = path.with_extension("submit.json");
            fs::copy(probe.root.join("submit.json"), &trace).unwrap();
            fs::set_permissions(trace, fs::Permissions::from_mode(0o600)).unwrap();
        }
        eprintln!("relay failure: {}", path.display());
    }
    Command::new("herdr")
        .args(["session", "stop", &probe.name, "--json"])
        .output()
        .unwrap();
    drop(terminal);
    native_controller_support::cleanup(&probe.root);
    assert_eq!(evidence["passed"], true, "{evidence}");
    retain_version_failure(&versions, &evidence, reviewer_notified);
    for (n, agent) in ["codex", "claude"].iter().enumerate() {
        versions[n].verify(&evidence["versions"][agent]);
    }
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(evidence_name(reviewer_notified)),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
fn retain_version_failure(versions: &[version::Version; 2], evidence: &Value, notified: bool) {
    if ["codex", "claude"]
        .iter()
        .enumerate()
        .any(|(i, agent)| !versions[i].matches(&evidence["versions"][agent]))
    {
        let mut retained = evidence.clone();
        retained["passed"] = false.into();
        retained["failure"] =
            "NATIVE_VERSION_CHANGED: provider journeys completed, exact-version proof unvalidated"
                .into();
        let root = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
        fs::write(
            root.join(format!("version-change-{}", evidence_name(notified))),
            serde_json::to_vec_pretty(&retained).unwrap(),
        )
        .unwrap();
    }
}
fn evidence_name(reviewer_notified: bool) -> &'static str {
    [
        ("KAH_GATE_COMMIT_SCOPE", "commit-scope.json"),
        ("KAH_GATE_VERIFY_FIRST", "verification-first.json"),
        ("KAH_GATE_QUEUE_ORDER", "queue-order.json"),
        ("KAH_GATE_AMBIGUITY", "genuine-ambiguity.json"),
        ("KAH_GATE_HOSTED_SEARCH", "hosted-search.json"),
        ("KAH_GATE_CHECKPOINTS", "cumulative-checkpoints.json"),
        ("KAH_GATE_UNCERTAIN_STEER", "uncertain-steering.json"),
        ("KAH_GATE_LINKED_STEER", "linked-steering.json"),
        ("KAH_GATE_REPLAY", "historical-replay.json"),
        ("KAH_GATE_REVIEW_DRIFT", "verified-drift.json"),
        ("KAH_GATE_REVIEW", "persistent-handoff.json"),
    ]
    .into_iter()
    .find(|(flag, _)| std::env::var_os(flag).is_some())
    .map(|(_, name)| name)
    .unwrap_or(if reviewer_notified {
        "user-steer.json"
    } else {
        "relay-dispatch.json"
    })
}

fn wait_evidence(root: &std::path::Path, terminal: &mut pty::Terminal) -> (Value, bool) {
    let mut notified = false;
    loop {
        terminal.respond_to_cursor_queries();
        if !notified && root.join("steer-ready.json").exists() {
            notify_reviewer(root);
            notified = true;
        }
        if let Ok(bytes) = fs::read(root.join("result.json")) {
            return (serde_json::from_slice(&bytes).unwrap(), notified);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn notify_reviewer(root: &std::path::Path) {
    let data: Value =
        serde_json::from_slice(&fs::read(root.join("steer-ready.json")).unwrap()).unwrap();
    let commands = ["codex", "claude"]
        .map(|agent| {
            format!(
                "kah steer {} {}",
                data[agent].as_str().unwrap(),
                pty::quote(data["text"].as_str().unwrap())
            )
        })
        .join("\n");
    let message = format!(
        "[kah-builder] 5O-0 live fixture ready on a PRIVATE Herdr server. Please run these two host commands from your named kah-reviewer pane (new kah installed). This tests your relayer attribution; no --sender or registration needed.\n{commands}\nBoth are disposable embedded kah workers. I will verify their ledgers/tripwire and clean them up. Please send now; fixture waits 10 minutes."
    );
    assert!(
        Command::new("herdr")
            .args(["agent", "prompt", "kah-reviewer", &message])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn relay_probe_entrypoint() {
    let Some(root) = std::env::var_os("KAH_RELAY_PROBE").map(PathBuf::from) else {
        return;
    };
    let result = std::panic::catch_unwind(|| relay_support::run(&root));
    let result = result.unwrap_or_else(|_| json!({"passed":false}));
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}
