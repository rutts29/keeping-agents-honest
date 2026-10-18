use super::*;
use kah_codex::control::Control;
use std::path::Path;

pub(super) fn prove(home: &Path, f: &fixture::Fixture, id: &str) {
    let index = f.overlay.join("session_index.jsonl");
    let global = home.join(".codex/session_index.jsonl");
    assert_eq!(fs::read_link(&index).unwrap(), global);
    assert_eq!(name(&global, id), "My own session");
    // Reproduce the directory-entry replacement used by native delete's index
    // rewrite. Then use the real native naming API against the forked store.
    let temporary = f.overlay.join("fixture-index.tmp");
    fs::copy(&index, &temporary).unwrap();
    fs::rename(temporary, &index).unwrap();
    let mut reader = Control::connect(
        &home.join(".local/bin/codex"),
        &f.overlay,
        &f.project,
        Duration::from_secs(20),
    )
    .unwrap();
    reader
        .request(
            "thread/name/set",
            json!({"threadId":id,"name":"Newest overlay fixture name"}),
        )
        .unwrap();
    drop(reader);
    assert!(fs::symlink_metadata(&index).unwrap().is_file());
    assert_eq!(name(&global, id), "My own session");
    let args = kah_cli::Arguments::parse(vec!["--".into(), "resume".into(), id.into()]).unwrap();
    let launch = kah_cli::Launch::prepare(home, &f.project, args).unwrap();
    assert_eq!(launch.overlay, f.overlay);
    assert_eq!(fs::read_link(&index).unwrap(), global);
    assert_eq!(name(&global, id), "Newest overlay fixture name");
    let mut command = launch.command(false);
    command.arg("--no-alt-screen");
    let mut terminal = pty::Terminal::start(&pty::command_line(&command), &f.stderr);
    answer_folder_access(&mut terminal);
    drop(terminal);
    assert_eq!(fs::read_link(index).unwrap(), global);
    assert_eq!(name(&global, id), "Newest overlay fixture name");
}
fn name(path: &Path, id: &str) -> String {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|row| row["id"] == id)
        .unwrap()["thread_name"]
        .as_str()
        .unwrap()
        .into()
}

/// Codex 0.162.1 can show "Folder access" on resume, and a key sent while the
/// TUI is still loading is dropped. Answer only that prompt, and only while it
/// is the last thing shown, until the resumed session prints STILL_NAMED.
fn answer_folder_access(terminal: &mut pty::Terminal) {
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut sent = None::<Instant>;
    loop {
        terminal.respond_to_cursor_queries();
        let log = terminal.output();
        if log.contains("STILL_NAMED") {
            return;
        }
        let due = sent.is_none_or(|at| at.elapsed() >= Duration::from_secs(2));
        if due && dialog::prompt_pending(&log, "Open restricted", "Ask Codex to do anything") {
            terminal.send("\r");
            sent = Some(Instant::now());
        }
        assert!(
            Instant::now() < deadline,
            "terminal did not show STILL_NAMED: {log}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
