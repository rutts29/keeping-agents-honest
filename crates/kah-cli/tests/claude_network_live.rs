#[allow(dead_code)]
mod claude_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
use claude_support::Fixture;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "native Claude hosted search and online MCP while Bash stays offline"]
fn claude_search_and_mcp_with_offline_bash() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".claude/settings.json")).unwrap();
    let version = Command::new("claude").arg("--version").output().unwrap();
    let f = Fixture::new_domain("untrusted");
    fs::write(
        f.project().join("fixture-server.js"),
        include_str!("native_extensions_support/server.js"),
    )
    .unwrap();
    let mcp = fixture_mcp(&f, &home);
    let mut terminal = f.start_with(
        false,
        "haiku",
        "low",
        &[
            "--mcp-config",
            &mcp,
            "--allowedTools",
            "Bash,mcp__fixture__probe,WebSearch",
        ],
    );
    f.ready(&mut terminal);
    let task = "steer: Use Bash once: curl --max-time 8 -Is https://example.com; echo SHELL_CURL_EXIT=$?. Then invoke fixture MCP probe once. Then perform actual hosted WebSearch for IANA example domains documentation. No WebFetch or other tools, files, subagents or installs. Summarize and stop.";
    f.prompt(&mut terminal, task);
    f.wait(&mut terminal, || {
        f.project().join("mcp-proof.json").is_file()
    });
    f.settled(&mut terminal, task);
    let rows = f.rows();
    let parts = rows
        .iter()
        .flat_map(|v| v["message"]["content"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    assert!(
        parts
            .iter()
            .any(|p| p["type"] == "tool_use" && p["name"] == "WebSearch"),
        "hosted search not invoked"
    );
    assert!(
        parts
            .iter()
            .filter(|p| p["type"] == "tool_result")
            .any(|p| p["content"].to_string().contains("SHELL_CURL_EXIT=6")),
        "Bash egress not blocked"
    );
    let proof: Value =
        serde_json::from_slice(&fs::read(f.project().join("mcp-proof.json")).unwrap()).unwrap();
    assert_eq!(proof["networkExit"], 0);
    assert_eq!(proof["hostLoginHidden"], true);
    drop(terminal);
    assert!(
        before == fs::read(home.join(".claude/settings.json")).unwrap(),
        "global settings changed"
    );
    let evidence = f.evidence.clone();
    drop(f);
    fs::write(evidence.join("claude-network.json"),serde_json::to_vec_pretty(&json!({"version":String::from_utf8(version.stdout).unwrap().trim(),"fixtureModel":"haiku","fixtureEffort":"low","domain":"untrusted-offline","hostedSearch":true,"bashOffline":true,"onlineMcp":proof,"globalSettingsUnchanged":true,"passed":true})).unwrap()).unwrap();
}

fn fixture_mcp(f: &Fixture, home: &std::path::Path) -> String {
    let fixture_config = f.project().join("fixture-global");
    fs::create_dir(&fixture_config).unwrap();
    let original = json!({"mcpServers":{"fixture":{"command":"/usr/bin/node","args":[f.project().join("fixture-server.js"),home.join(".codex/auth.json")]}}});
    fs::write(fixture_config.join(".claude.json"), original.to_string()).unwrap();
    let (servers, _) =
        kah_cli::claude_setup::mcp_configuration(&fixture_config, &Default::default()).unwrap();
    json!({"mcpServers":servers}).to_string()
}
