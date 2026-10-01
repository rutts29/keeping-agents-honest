use super::*;
use kah_cli::{native_history::History, run_pane::Pane};

pub fn prove(home: &Path, root: &Path, id: &str, record: &Record) -> Value {
    let native = record.native.as_ref().unwrap();
    let session = native.session.as_deref().unwrap();
    let directory = Store::path(&RunStore::default_state_root().unwrap().join("runs"), id).unwrap();
    let mut history = History::open(
        home,
        &directory,
        &native.provider,
        &record.worktree,
        session,
    )
    .unwrap();
    let transcript = match &mut history {
        History::Claude { path, .. } => path.clone(),
        History::Codex { client, .. } => PathBuf::from(
            client
                .request(
                    "thread/read",
                    json!({"threadId":session,"includeTurns":false}),
                )
                .unwrap()["thread"]["path"]
                .as_str()
                .unwrap(),
        ),
    };
    drop(history);
    assert!(transcript.is_file());
    let bytes = fs::metadata(&transcript).unwrap().len();
    assert_eq!(record.worktree.parent().unwrap().file_name().unwrap(), id);
    assert!(
        record
            .project
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("kah-native-run-recovery-")
    );
    fs::remove_dir_all(&record.worktree).unwrap();
    assert!(transcript.is_file());
    assert_eq!(fs::metadata(&transcript).unwrap().len(), bytes);
    if native.provider == "claude" {
        let mut cleanup: Value =
            serde_json::from_slice(&fs::read(root.join("cleanup.json")).unwrap()).unwrap();
        cleanup["extraClaudeProject"] = true.into();
        fs::write(root.join("cleanup.json"), cleanup.to_string()).unwrap();
    }
    let command = viewer(home, &directory, record, session);
    let pane = Pane::start(&command).unwrap();
    recovery::operator_herdr(&["pane", "zoom", &pane.id, "--on"]);
    let result = std::panic::catch_unwind(|| wait_view(&pane, &native.provider));
    pane.close().unwrap();
    // Restore only the empty fixture directory so native exact-path purge can
    // resolve it; the old worktree contents are deliberately still absent.
    fs::create_dir_all(&record.worktree).unwrap();
    result.unwrap();
    assert!(transcript.is_file());
    json!({"worktreeRemoved":true,"transcriptRetained":true,"nativeResumeOpenedHistory":true,"resumeFrom":"original source repository","sameSessionIdArgument":true,"noModelPromptSent":true,"codexViewUsesOwnedInspectionHome":native.provider=="codex"})
}
fn viewer(home: &Path, directory: &Path, record: &Record, session: &str) -> Command {
    let provider = &record.native.as_ref().unwrap().provider;
    let mut command = Command::new(home.join(".local/bin").join(provider));
    command
        .current_dir(&record.project)
        .env_remove("CODEX_THREAD_ID")
        .env_remove("KAH_OBSERVER_BINDING")
        .env_remove("KAH_CLAUDE_BINDING")
        .env_remove("KAH_SESSION_POLICY")
        .env_remove("KAH_RUN_ID")
        .env_remove("KAH_RUN_INSTANCE");
    if provider == "claude" {
        command.args(["--setting-sources", "user", "--resume", session]);
    } else {
        let inspection = directory.join("inspection");
        kah_codex::overlay::write_settings(&inspection,json!({"projects":{record.project.to_string_lossy().as_ref():{"trust_level":"untrusted"}}})).unwrap();
        command
            .env("CODEX_HOME", inspection)
            .args(["--no-daemon", "--cd"])
            .arg(&record.project)
            .args(["resume", session]);
    }
    command
}
fn wait_view(pane: &Pane, provider: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut consent = false;
    loop {
        let screen = pane.logs(provider, None, None).unwrap();
        if !consent
            && (screen.contains("Open restricted") || screen.contains("Yes, I trust this folder"))
        {
            native_consent(&pane.id, provider);
            consent = true;
        }
        if screen.contains("preserved.txt") || screen.contains("resumed.txt") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native history did not reopen after worktree cleanup: {screen}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
