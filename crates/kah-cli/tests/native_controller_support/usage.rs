use super::*;
use kah_cli::native_history::History;
use std::{collections::BTreeSet, io::BufReader};

pub fn run(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = global_settings(&home);
    let project = fixture_project(
        &home,
        &format!("kah-native-run-usage-{agent}-{}", std::process::id()),
        Domain::Research,
    );
    let task = "This is an authorized native subagent fixture. Start exactly one native subagent and ask it to reply CHILD_OK without using tools. Use claude-sonnet-5-5 for a Claude child, or gpt-6.1-sol if the Codex native tool exposes that choice. Wait for its reply. Then use your native file tool to create child-complete.txt containing DONE, and report. Do not push or deploy.";
    let output = launch(
        &home,
        &project.root,
        agent,
        fixture_model(agent),
        task,
        "test -f child-complete.txt && sleep 120",
        &[],
    );
    let id = output
        .lines()
        .find_map(|s| s.strip_prefix("Created "))
        .unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":agent}).to_string(),
    )
    .unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let counts = wait_usage(&home, root, &base, id, agent);
    recovery::operator(&["stop", id]);
    let record = recovery::wait_stopped(&base, id);
    assert_eq!(record.control.inputs().len(), 1);
    assert_eq!(record.control.task(), task);
    assert!(record.native.as_ref().unwrap().native_tokens >= counts["sum"].as_u64().unwrap());
    for (path, bytes) in before {
        assert_eq!(bytes, fs::read(path).unwrap());
    }
    json!({"agent":agent,"version":version(&home,agent),"fixtureModel":fixture_model(agent),"fixtureEffort":"low","sourceDomain":"research","consentRequired":true,"counts":counts,"childrenAreNotUserIntent":true,"accountedNativeTokens":record.native.unwrap().native_tokens,"globalConfigUnchanged":true,"passed":true})
}
fn wait_usage(home: &Path, root: &Path, base: &Path, id: &str, agent: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut accepted = false;
    loop {
        let record = Store::read(base, id).unwrap();
        let native = record.native.as_ref().unwrap();
        if let Some(pane) = native.pane.as_deref() {
            let screen = pane_screen(pane, native.session.as_deref(), agent);
            if !accepted
                && native.waiting.is_some()
                && (screen.contains("Open restricted")
                    || screen.contains("Yes, I trust this folder"))
            {
                accept_consent(id, pane, agent);
                accepted = true;
            }
            if record.worktree.join("child-complete.txt").exists() && native.source_cursor == 1 {
                let counts = measure(home, root, &Store::path(base, id).unwrap(), &record);
                if counts["children"].as_u64().unwrap() > 0
                    && native.native_tokens >= counts["sum"].as_u64().unwrap()
                {
                    return counts;
                }
            }
            assert!(
                Instant::now() < deadline,
                "child usage did not arrive: {screen}; {}",
                serde_json::to_string(&record).unwrap()
            );
        }
        assert!(Instant::now() < deadline, "usage controller did not start");
        std::thread::sleep(Duration::from_millis(500));
    }
}
fn measure(home: &Path, root: &Path, directory: &Path, record: &Record) -> Value {
    let native = record.native.as_ref().unwrap();
    let mut history = History::open(
        home,
        directory,
        &native.provider,
        &record.worktree,
        native.session.as_deref().unwrap(),
    )
    .unwrap();
    let (parent, children, ids) = match &mut history {
        History::Claude { path, session, .. } => {
            let children = fs::read_dir(path.parent().unwrap().join(session).join("subagents"))
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|s| s == "jsonl"))
                .collect();
            (path.clone(), children, vec![])
        }
        History::Codex {
            client, session, ..
        } => codex_files(client, session, &record.worktree),
    };
    if !ids.is_empty() {
        let mut cleanup: Value =
            serde_json::from_slice(&fs::read(root.join("cleanup.json")).unwrap()).unwrap();
        cleanup["children"] = json!(ids);
        fs::write(root.join("cleanup.json"), cleanup.to_string()).unwrap();
    }
    let parent_tokens = tokens(&parent, &native.provider);
    let child_tokens = children
        .iter()
        .map(|p| tokens(p, &native.provider))
        .sum::<u64>();
    let models = children
        .iter()
        .flat_map(|p| models(p))
        .collect::<BTreeSet<_>>();
    let snapshot = history.snapshot().unwrap();
    assert_eq!(snapshot.users.unwrap().authoritative_records().len(), 1);
    json!({"parent":parent_tokens,"children":child_tokens,"childFiles":children.len(),"childModels":models,"sum":parent_tokens+child_tokens})
}
fn codex_files(
    client: &mut kah_codex::control::Control,
    session: &str,
    project: &Path,
) -> (PathBuf, Vec<PathBuf>, Vec<String>) {
    let read = client
        .request(
            "thread/read",
            json!({"threadId":session,"includeTurns":false}),
        )
        .unwrap();
    let parent = PathBuf::from(read["thread"]["path"].as_str().unwrap());
    let mut children = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for archived in [false, true] {
        let page=client.request("thread/list",json!({"ancestorThreadId":session,"archived":archived,"limit":100,"sourceKinds":["subAgent","subAgentReview","subAgentCompact","subAgentThreadSpawn","subAgentOther"]})).unwrap();
        assert!(
            page["nextCursor"].is_null(),
            "single-child fixture unexpectedly paginated"
        );
        for child in page["data"].as_array().unwrap() {
            assert_eq!(child["cwd"].as_str(), project.to_str());
            if let Some(path) = child["path"].as_str() {
                children.insert(PathBuf::from(path));
            }
            ids.insert(child["id"].as_str().unwrap().to_owned());
        }
    }
    (
        parent,
        children.into_iter().collect(),
        ids.into_iter().collect(),
    )
}
fn tokens(path: &Path, provider: &str) -> u64 {
    let reader = BufReader::new(fs::File::open(path).unwrap());
    if provider == "claude" {
        kah_claude::usage::tokens(reader).unwrap()
    } else {
        kah_codex::usage::tokens(reader).unwrap()
    }
}
fn models(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|v| {
            v["message"]["model"]
                .as_str()
                .or(v["payload"]["model"].as_str())
                .map(str::to_owned)
        })
        .collect()
}
