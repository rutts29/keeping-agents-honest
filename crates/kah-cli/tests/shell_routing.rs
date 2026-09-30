use std::process::Command;

#[test]
fn management_commands_bypass_kah_even_after_global_options() {
    for command in [
        "app-server",
        "login",
        "logout",
        "update",
        "doctor",
        "features",
        "mcp",
        "plugin",
        "agents",
        "exec-server",
        "sandbox",
        "debug",
        "apply",
        "archive",
        "delete",
        "exec",
        "e",
        "review",
        "remote-control",
        "completion",
        "queue",
        "migrate-rollouts",
        "unarchive",
        "cloud",
        "a",
        "help",
    ] {
        for prefix in [vec![], vec!["--config", "model=chosen"], vec!["-mchosen"]] {
            let mut args = prefix;
            args.push(command);
            assert!(native(&args), "{args:?}");
        }
    }
}

#[test]
fn prompts_option_values_resume_and_fork_remain_interactive() {
    for args in [
        vec![],
        vec!["resume", "--last"],
        vec!["fork", "id"],
        vec!["--model", "login"],
        vec!["--", "login"],
        vec!["fix this", "login"],
        vec!["--config=model=login", "fix this"],
        vec!["--no-daemon", "resume"],
    ] {
        assert!(!native(&args), "{args:?}");
    }
    assert!(native(&["--version"]));
    assert!(native(&["--help"]));
}

fn native(args: &[&str]) -> bool {
    kah_cli::shell_routing::is_native_command(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
}

#[test]
#[ignore = "installed Bash routing and native daemon version; no restart or configuration writes"]
fn installed_management_routing() {
    let home = std::env::var("HOME").unwrap();
    let mut results = vec![];
    for domain in ["mine", "research", "untrusted"] {
        let output = Command::new("bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "source \"$HOME/.local/share/kah/shell.sh\"; codex app-server daemon version",
            ])
            .current_dir(format!("{home}/repos/{domain}"))
            .env_remove("CODEX_HOME")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let version: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        results.push(serde_json::json!({"domain":domain,"nativeDaemonVersion":version}));
    }
    let path = std::path::PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    std::fs::write(
        path.join("management-routing.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
