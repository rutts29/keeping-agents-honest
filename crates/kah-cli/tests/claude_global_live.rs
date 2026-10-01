#[allow(dead_code)]
mod claude_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
use claude_support::Fixture;
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "real restricted Claude launcher with global setup, native subagent and domain history"]
fn claude_global_setup_in_both_restricted_domains() {
    for domain in ["research", "untrusted"] {
        journey(domain);
    }
}
fn journey(domain: &str) {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".claude/settings.json")).unwrap();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let f = Fixture::new_domain(domain);
    let config = home.join(".claude");
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    let skill = home.join(".agents/skills/ponytail/SKILL.md");
    let task = format!(
        "Please run this disposable full-setup test. Read the real installed ponytail skill at {} and follow its minimal-change guidance. Use native Write to create parent.txt containing PARENT_OK. Spawn exactly one native subagent for this authorized journey only; it must read the same global skill, use Bash to run `test ! -e /home/example/.codex/auth.json && test ! -e /home/example/.claude/.credentials.json && echo CHILD_LOGINS_HIDDEN; curl --max-time 8 -Is https://example.com; echo CHILD_CURL_EXIT=$?`, and use native Write to create child.txt containing CHILD_OK. Wait for the child. No other subagents, installs, commits or permission workarounds. Report and stop.",
        skill.display()
    );
    f.prompt(&mut terminal, &task);
    f.wait(&mut terminal, || {
        (f.has_file("parent.txt", "PARENT_OK") && f.has_file("child.txt", "CHILD_OK"))
            || asks_confirmation(&f)
    });
    if asks_confirmation(&f) {
        f.prompt(&mut terminal, "Yes. I intentionally requested this disposable test in my preceding user message. The kah reminder repeats that request. Please perform that test.");
        f.wait(&mut terminal, || {
            f.has_file("parent.txt", "PARENT_OK") && f.has_file("child.txt", "CHILD_OK")
        });
    }
    f.settled(&mut terminal, &task);
    f.wait(&mut terminal, || {
        f.rows().iter().any(|row| {
            let text = row["message"]["content"].to_string();
            text.contains("<task-notification>") && text.contains("completed")
        })
    });
    child_proof(&f, &config);
    drop(terminal);
    assert!(
        before == fs::read(home.join(".claude/settings.json")).unwrap(),
        "global settings changed"
    );
    let evidence = f.evidence.clone();
    drop(f);
    fs::write(evidence.join(format!("claude-global-{domain}.json")),serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),"fixtureModel":"haiku","fixtureEffort":"low","domain":domain,"nativeLauncher":true,"nativeProjectHistory":true,"singleNativeCredentialStore":true,"nativeGlobalSetup":true,"globalSkillParentAndChild":true,"nativeWrites":true,"childShellLoginsHidden":true,"childShellOffline":true,"globalSettingsUnchanged":true,"passed":true})).unwrap()).unwrap();
}

fn child_proof(f: &Fixture, config: &std::path::Path) {
    let child_root = config
        .join("projects")
        .join(f.project().to_string_lossy().replace('/', "-"))
        .join(&f.id)
        .join("subagents");
    let files = fs::read_dir(&child_root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|s| s == "jsonl"))
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let child = fs::read_to_string(files[0].path()).unwrap();
    let rows = child
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect::<Vec<_>>();
    let diagnostic =
        std::env::temp_dir().join(format!("kah-claude-child-{}.jsonl", std::process::id()));
    use std::os::unix::fs::PermissionsExt;
    fs::write(&diagnostic, &child).unwrap();
    fs::set_permissions(&diagnostic, fs::Permissions::from_mode(0o600)).unwrap();
    let results = rows
        .iter()
        .flat_map(|v| v["message"]["content"].as_array().into_iter().flatten())
        .filter(|p| p["type"] == "tool_result")
        .map(|p| p["content"].to_string())
        .collect::<String>();
    assert!(
        results.contains("CHILD_LOGINS_HIDDEN") && results.contains("CHILD_CURL_EXIT=6"),
        "native child tool result did not prove containment; private diagnostic: {}",
        diagnostic.display()
    );
    assert!(
        rows.iter().any(|v| v["message"]["content"]
            .to_string()
            .contains("ponytail/SKILL.md")),
        "child global skill access absent"
    );
    fs::remove_file(diagnostic).unwrap();
}

fn asks_confirmation(f: &Fixture) -> bool {
    f.rows()
        .iter()
        .filter(|r| r["type"] == "assistant")
        .any(|r| {
            let text = r["message"]["content"].to_string();
            text.contains("Did you intentionally ask") || text.contains("need to confirm")
        })
}
