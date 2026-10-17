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
use std::{fs, process::Command, time::Duration};

#[test]
#[ignore = "native session-only effort control on an effort-capable model"]
fn claude_native_effort_choice() {
    let f = Fixture::new();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let settings_home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = claude_support::settings::snapshot(&settings_home);
    let mut terminal = f.start_with(false, "claude-sonnet-5-5", "low", &[]);
    f.ready(&mut terminal);
    let offset = terminal.log_size();
    terminal.submit("/effort");
    f.wait_text(&mut terminal, "session", offset);
    std::thread::sleep(Duration::from_millis(350));
    terminal.send("\x1b[C");
    std::thread::sleep(Duration::from_millis(250));
    terminal.send("s");
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        before == claude_support::settings::snapshot(&settings_home),
        "native session-only effort changed global settings"
    );
    let prompt = "Reply exactly EFFORT_OK without using tools.";
    f.prompt(&mut terminal, prompt);
    f.wait(&mut terminal, || {
        f.rows().iter().any(|r| {
            r["type"] == "assistant" && r["message"]["content"].to_string().contains("EFFORT_OK")
        })
    });
    f.settled(&mut terminal, prompt);
    let row = f
        .rows()
        .into_iter()
        .rfind(|r| r["type"] == "assistant")
        .unwrap();
    assert!(row["message"]["model"].as_str().unwrap().contains("sonnet"));
    assert!(
        row["effort"].to_string().contains("medium")
            || row["perTurnEffort"].to_string().contains("medium"),
        "effort fields: {} / {}",
        row["effort"],
        row["perTurnEffort"]
    );
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    assert!(
        before == claude_support::settings::snapshot(&settings_home),
        "global settings changed"
    );
    fs::write(f.evidence.join("effort.json"), serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),
        "passed":true,"fixtureInitialModel":"claude-sonnet-5-5","fixtureInitialEffort":"low","nativeEffortControl":"/effort slider, Right then s (session only)",
        "observedModel":row["message"]["model"],"observedEffort":row["effort"],"observedPerTurnEffort":row["perTurnEffort"],"globalSettingsUnchanged":true})).unwrap()).unwrap();
}

#[test]
#[ignore = "native mid-session model picker using session-only selection; no inference after switching"]
fn claude_native_model_switch() {
    let f = Fixture::new();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let settings_home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = claude_support::settings::snapshot(&settings_home);
    let capture = f.project().join("model-change.json");
    fs::create_dir(f.project().join(".claude")).unwrap();
    let hook = format!("cat > {}", pty::quote(capture.to_str().unwrap()));
    fs::write(
        f.project().join(".claude/settings.local.json"),
        json!({"hooks":{"PostModelSwitch":[{"hooks":[{"type":"command","command":hook}]}]}})
            .to_string(),
    )
    .unwrap();
    let mut terminal = f.start(false);
    f.ready(&mut terminal);
    let prompt = "Use native Write to create before-switch.txt containing SWITCH_BEFORE. No other tools or files; report and stop.";
    f.prompt(&mut terminal, prompt);
    f.wait(&mut terminal, || {
        f.has_file("before-switch.txt", "SWITCH_BEFORE")
    });
    f.settled(&mut terminal, prompt);
    let assistant_before = f.rows().iter().filter(|r| r["type"] == "assistant").count();
    let offset = terminal.log_size();
    terminal.submit("/model");
    f.wait_text(&mut terminal, "session", offset);
    f.wait_text(&mut terminal, "Sonnet 5.5 ✔", offset);
    std::thread::sleep(Duration::from_millis(350));
    f.move_cursor_to(&mut terminal, "Opus 5.5", offset);
    terminal.send("s");
    f.wait_text(&mut terminal, "Switch model?", offset);
    f.wait_text(&mut terminal, "Yes, switch to Opus", offset);
    std::thread::sleep(Duration::from_millis(350));
    terminal.send("\r");
    f.wait(&mut terminal, || {
        model_event(&capture).is_some_and(|v| v["from_model"] != v["to_model"])
    });
    let event = model_event(&capture).unwrap();
    assert!(event["to_model"].as_str().unwrap().contains("opus"));
    assert_eq!(event["session_id"], f.id);
    assert_eq!(event["hook_event_name"], "PostModelSwitch");
    terminal.submit("/exit");
    terminal.wait_exit(false);
    drop(terminal);
    assert!(
        before == claude_support::settings::snapshot(&settings_home),
        "session-only model switch changed global settings"
    );
    assert_eq!(
        assistant_before,
        f.rows().iter().filter(|r| r["type"] == "assistant").count()
    );
    fs::write(f.evidence.join("model-switch.json"), serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),
        "passed":true,"nativeTaskBeforeSwitch":true,"fromModel":event["from_model"],"toModel":event["to_model"],
        "nativeModelControl":"/model picker, Opus 5.5 row chosen by label then s (session only), native cache confirmation","inferenceAfterSwitch":false,"globalSettingsUnchanged":true})).unwrap()).unwrap();
}

fn model_event(path: &std::path::Path) -> Option<serde_json::Value> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
