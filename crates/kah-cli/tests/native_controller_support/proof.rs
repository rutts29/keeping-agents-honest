use super::*;

pub(super) const VERIFY: &str = "node -e 'const a=require(\"node:assert/strict\"), f=require(\"node:fs\"), double=require(\"./double.js\"); a.equal(double(21),42); a.equal(double(-3),-6); a.ok(!f.existsSync(\"/home/example/.codex/auth.json\")); a.ok(!f.existsSync(\"/home/example/.claude/.credentials.json\")); console.log(\"FUNCTION_OK LOGIN_HIDDEN\")'";

pub(super) fn native_tools(home: &Path, directory: &Path, record: &Record) {
    let native = record.native.as_ref().unwrap();
    let mut history = kah_cli::native_history::History::open(
        home,
        &directory.join("fixture-proof"),
        &native.provider,
        &record.worktree,
        native.session.as_ref().unwrap(),
    )
    .unwrap();
    let actions = history.completion_activity().unwrap();
    if native.provider == "codex" {
        assert!(
            actions
                .iter()
                .any(|a| a["kind"] == "fileChange" && a["status"] == "completed")
        );
        assert!(
            actions
                .iter()
                .any(|a| a["kind"] == "commandExecution" && a["exit_code"] == 0)
        );
    } else {
        let messages = actions
            .iter()
            .filter_map(|a| a["evidence"].as_str())
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .collect::<Vec<_>>();
        let tools = messages
            .iter()
            .flat_map(|m| m["content"].as_array().into_iter().flatten())
            .filter(|p| p["type"] == "tool_use")
            .filter_map(|p| p["name"].as_str())
            .collect::<Vec<_>>();
        assert!(tools.contains(&"Write") || tools.contains(&"Edit"));
        assert!(tools.contains(&"Bash"));
    }
    // The absence checks have real host files as their positive counterpart.
    assert!(home.join(".codex/auth.json").is_file());
    assert!(home.join(".claude/.credentials.json").is_file());
}
