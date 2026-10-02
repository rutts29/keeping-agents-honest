use super::*;
use kah_run::{access, verification, worktree::Worktree};
use kah_sandbox::LaunchOptions;

struct Fixture(PathBuf);
struct Cwd(PathBuf);
impl Drop for Cwd {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        cleanup(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn verification_uses_imported_commit() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let (_fixture, project, id, directory) = setup(&home);
    let worker = Worktree::load(&directory.join("worktree.json")).unwrap();
    let _cwd = Cwd(std::env::current_dir().unwrap());
    std::env::set_current_dir(&worker.path).unwrap();
    fs::write(worker.path.join("marker.txt"), "NATIVE_RUN_OK\n").unwrap();
    let worker_executor = executor(&home, &worker);
    let bundle = directory.join("diagnostic.bundle");
    commit_fixture(&worker_executor, &worker);
    fs::write(
        worker.path.join("marker.txt"),
        "UNCOMMITTED_NOT_DELIVERED\n",
    )
    .unwrap();
    let commit = worker.bundle_head(&worker_executor, &bundle).unwrap();
    let candidate = Worktree::create(
        &project.root,
        &worker.path.parent().unwrap().join("candidate"),
        &id,
    )
    .unwrap();
    candidate.import_bundle(&bundle, &commit).unwrap();
    assert_eq!(
        fs::read_to_string(candidate.path.join("marker.txt")).unwrap(),
        "NATIVE_RUN_OK\n"
    );
    let tree = candidate
        .verification_snapshot(
            &candidate.path.parent().unwrap().join("verify-diagnostic"),
            &commit,
        )
        .unwrap();
    let uid = kah_run::access::service_uid().unwrap();
    access::traverse(candidate.path.parent().unwrap(), uid).unwrap();
    access::share_tree(&tree.path, uid, true).unwrap();
    let verify = executor(&home, &tree)
        .with_read_only_mounts(&[
            (tree.git_dir.clone(), tree.git_dir.clone()),
            (tree.path.join(".git"), tree.path.join(".git")),
        ])
        .unwrap();
    let receipt = verification::execute(
        &verify,
        "pwd; ls -a; test \"$(cat marker.txt)\" = NATIVE_RUN_OK",
        &commit,
        Duration::from_secs(20),
        &directory.join("diagnostic.log"),
    )
    .unwrap();
    assert!(receipt.success);
    tree.remove_verification().unwrap();
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        evidence.join("verification-path.json"),
        serde_json::to_vec_pretty(
            &json!({"modelCalls":0,"candidateContainsMarker":true,"uncommittedCodeExcluded":true,"kahAuthoredCommits":false,"receipt":receipt,"passed":true}),
        )
        .unwrap(),
    )
    .unwrap();
}
fn commit_fixture(executor: &kah_sandbox::Executor, worker: &Worktree) {
    let mut commit = executor.command(&worker.path, &["/bin/sh".into(),"-c".into(),"git -c safe.directory=\"$PWD\" add marker.txt && git -c safe.directory=\"$PWD\" -c core.hooksPath=/dev/null -c commit.gpgsign=false commit -m 'fixture agent delivery'".into()]).unwrap();
    assert!(commit.status().unwrap().success());
}
fn setup(home: &Path) -> (Fixture, Project, String, PathBuf) {
    let root = std::env::temp_dir().join(format!("kah-native-checkpoint-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let name = format!("kah-native-run-checkpoint-{}", std::process::id());
    let project = Project::select(&home.join("repos"), home, Some((Domain::Mine, &name))).unwrap();
    assert!(project.needs_creation);
    project.initialize().unwrap();
    seed(&project.root);
    let args = kah_cli::unattended_args::RunArguments::parse(
        ["--test", "true", "checkpoint fixture"]
            .map(str::to_owned)
            .to_vec(),
    )
    .unwrap();
    let id = kah_cli::unattended_setup::create(home, &project.root, args).unwrap();
    fs::write(
        root.join("cleanup.json"),
        json!({"run":id,"source":project.root,"agent":"codex"}).to_string(),
    )
    .unwrap();
    let directory =
        Store::path(&RunStore::default_state_root().unwrap().join("runs"), &id).unwrap();
    (Fixture(root), project, id, directory)
}
pub(super) fn executor(home: &Path, tree: &Worktree) -> kah_sandbox::Executor {
    let project = Project {
        domain: Domain::Research,
        root: tree.path.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    kah_cli::native_setup::executor(
        home,
        &project,
        LaunchOptions::default(),
        Some(&tree.git_dir),
    )
    .unwrap()
}
