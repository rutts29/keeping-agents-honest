use super::*;

pub(super) fn native_name(home: &Path, directory: &Path, id: &str, record: &Record) -> Value {
    let native = record.native.as_ref().unwrap();
    let expected = native.name.as_deref().expect("native run name");
    if native.provider == "claude" {
        let pid = super::recovery::native_pid(super::recovery::controller(id, record), "claude");
        let entry: Value = serde_json::from_slice(
            &fs::read(home.join(".claude/sessions").join(format!("{pid}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(entry["name"], expected);
        json!({"name":expected,"source":"native --name and sessions/<pid>.json","advisorySessionId":entry["sessionId"],"hookSessionId":native.confirmation.as_ref().map(|c|&c.session)})
    } else {
        let mut history = kah_cli::native_history::History::open(
            home,
            directory,
            &native.provider,
            &record.worktree,
            native.session.as_deref().unwrap(),
        )
        .unwrap();
        let kah_cli::native_history::History::Codex {
            ref mut client,
            ref session,
            ..
        } = history
        else {
            unreachable!()
        };
        let thread = client
            .request(
                "thread/read",
                json!({"threadId":session,"includeTurns":false}),
            )
            .unwrap();
        assert_eq!(thread["thread"]["name"], expected);
        json!({"name":expected,"source":"native thread/name/set and thread/read","session":session})
    }
}

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
        claude_tools(&history);
    }
    // The absence checks have real host files as their positive counterpart.
    assert!(home.join(".codex/auth.json").is_file());
    assert!(home.join(".claude/.credentials.json").is_file());
}

fn claude_tools(history: &kah_cli::native_history::History) {
    let kah_cli::native_history::History::Claude { path, session, .. } = history else {
        unreachable!()
    };
    // Completion intentionally sees only recent actions. Tool-parity proof must
    // inspect the whole primary transcript: later commits can evict the first Write.
    let text = fs::read_to_string(path).unwrap();
    let rows = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            row["type"] == "assistant" && row["sessionId"] == *session && row["isSidechain"] != true
        })
        .collect::<Vec<_>>();
    let tools = rows
        .iter()
        .flat_map(|row| row["message"]["content"].as_array().into_iter().flatten())
        .filter(|part| part["type"] == "tool_use")
        .filter_map(|part| part["name"].as_str())
        .collect::<Vec<_>>();
    assert!(tools.contains(&"Write") || tools.contains(&"Edit"));
    assert!(tools.contains(&"Bash"));
}

pub(super) fn completed(record: &Record, task: &str) {
    assert_eq!(
        record.control.status(),
        RunStatus::Completed,
        "{}",
        serde_json::to_string(&record).unwrap()
    );
    assert_eq!(record.control.task(), task);
    assert!(record.control.tokens_used() > 0);
    let native = record.native.as_ref().unwrap();
    assert!(
        native.waiting.is_none(),
        "consent wait must clear after submission"
    );
    assert!(native.native_tokens > 0 && native.observer_tokens > 0);
    assert!(native.synchronized());
}
