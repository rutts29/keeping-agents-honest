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
fn newly_installed_commands_are_discovered_without_rerouting_resume_or_prompts() {
    let help = "Usage: codex [OPTIONS]\n\nCommands:\n  future-admin  New management\n                task description continued\n  resume  Resume\n  fork  Fork\n\nOptions:\n  --help  Help\n";
    for (arg, expected) in [
        ("future-admin", true),
        ("resume", false),
        ("fork", false),
        ("task", false),
    ] {
        assert_eq!(
            kah_cli::shell_routing::with_help(&[arg.into()], Some(help)),
            expected
        );
    }
    assert!(kah_cli::shell_routing::with_help(
        &["login".into()],
        Some("unparseable")
    ));
}

#[test]
#[ignore = "installed Bash routing and native restart help in a disposable CODEX_HOME"]
fn installed_management_routing() {
    let home = std::env::var("HOME").unwrap();
    let mut results = vec![];
    let isolated = std::env::temp_dir().join(format!("kah-routing-{}", std::process::id()));
    std::fs::create_dir_all(&isolated).unwrap();
    for domain in ["mine", "research", "untrusted"] {
        let output = Command::new("bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "source \"$HOME/.local/share/kah/shell.sh\"; codex app-server daemon restart --help",
            ])
            .current_dir(format!("{home}/repos/{domain}"))
            .env("CODEX_HOME", &isolated)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("codex app-server daemon restart")
        );
        results.push(serde_json::json!({"domain":domain,"nativeRestartHelp":true,"isolatedHome":true,"daemonRestarted":false}));
    }
    let path = std::path::PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    std::fs::write(
        path.join("management-routing.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    std::fs::remove_dir_all(isolated).unwrap();
}
