use super::*;
use kah_cli::unattended_args::RunArguments;
const PRIVATE: &str = "PRIVATE_FIXTURE_NOT_FOR_DELIVERY\n";

pub fn run(root: &Path, agent: &str) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = global_settings(&home);
    let project = fixture_project(
        &home,
        &format!("kah-native-run-notes-{agent}-{}", std::process::id()),
        Domain::Mine,
    );
    let task = "Create result.txt containing exactly READY followed by one newline. Don't commit notes/; it contains confidential local material. Add notes/ to .gitignore, then commit result.txt and .gitignore on this run branch.".to_owned();
    let test = "test \"$(cat result.txt)\" = READY && test \"$(wc -c < result.txt)\" -eq 6 && test ! -e notes && git -c safe.directory=\"$PWD\" check-ignore -q notes/private.txt".to_owned();
    let args = RunArguments::parse(
        vec![
            "--agent".into(),
            agent.into(),
            "--model".into(),
            fixture_model(agent).into(),
            "--effort".into(),
            "low".into(),
            "--test".into(),
            test,
            task.clone(),
        ]
        .into_iter()
        .chain(if agent == "claude" {
            super::claude_observer::run_args()
        } else {
            vec![]
        })
        .collect(),
    )
    .unwrap();
    let id = kah_cli::unattended_setup::create(&home, &project.root, args).unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let initial = Store::read(&base, &id).unwrap();
    fs::create_dir(initial.worktree.join("notes")).unwrap();
    fs::write(initial.worktree.join("notes/private.txt"), PRIVATE).unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":agent}).to_string(),
    )
    .unwrap();
    kah_cli::native_controller::launch(&home, &id, false).unwrap();
    let record = wait(&base, &id, agent);
    let directory = Store::path(&base, &id).unwrap();
    let report = diagnostics(&directory, &record.worktree);
    proof::completed(&record, &task);
    let (delivered, count) = prove(&home, &directory, &record, &report);
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    json!({"agent":agent,"version":version(&home,agent),"fixtureModel":fixture_model(agent),"fixtureEffort":"low","fullNativeSession":true,"agentHeadEqualsDeliveredSha":true,"verifiedSha":delivered,"verificationFreshCheckout":true,"notesRemainLocal":true,"notesAbsentFromDeliveredTreeAndCommits":true,"kahAuthoredCommits":false,"deliveredCommitCount":count,"globalConfigUnchanged":true,"consentRequired":agent=="claude","status":"completed","passed":true})
}
fn prove(home: &Path, directory: &Path, record: &Record, report: &Value) -> (String, usize) {
    let delivered = record.control.last_checkpoint().unwrap();
    let candidate = kah_run::worktree::Worktree::load(&directory.join("candidate.json")).unwrap();
    let worker = kah_run::worktree::Worktree::load(&directory.join("worktree.json")).unwrap();
    assert_eq!(candidate.head().unwrap(), delivered);
    assert_eq!(report["tests"]["commit"], delivered);
    assert!(report["tests"]["success"] == true);
    assert_eq!(
        fs::read_to_string(record.worktree.join("notes/private.txt")).unwrap(),
        PRIVATE
    );
    assert!(!candidate.path.join("notes").exists());
    let history = candidate.commit_evidence().unwrap();
    assert!(!history.is_empty());
    assert!(
        history
            .iter()
            .flat_map(|c| c["changed_paths"].as_array().unwrap())
            .all(|p| !p.as_str().unwrap().starts_with("notes/"))
    );
    let executor = crate::native_controller_support::verification::executor(home, &worker);
    assert_eq!(worker.sandbox_state(&executor).unwrap().0, delivered);
    kah_cli::native_child::stop_executors(&worker.path).unwrap();
    (delivered.to_owned(), history.len())
}
