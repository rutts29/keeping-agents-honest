use kah_run::{access, verification, worktree::Worktree};
use kah_sandbox::{Domain, LaunchOptions, Project};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

pub(super) fn prove(
    home: &Path,
    source: &Path,
    worker: &Worktree,
    root: &Path,
    state: &kah_observer::session::State,
) -> Value {
    let project = Project {
        domain: Domain::Research,
        root: worker.path.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let executor = kah_cli::native_setup::executor(
        home,
        &project,
        LaunchOptions::default(),
        Some(&worker.git_dir),
    )
    .unwrap();
    let bundle = root.join("candidate.bundle");
    let commit = worker.checkpoint_bundle(&executor, &bundle).unwrap();
    let candidate = Worktree::create(source, &root.join("candidate"), "run-native-pane").unwrap();
    candidate.import_bundle(&bundle, &commit).unwrap();
    assert_eq!(candidate.checkpoint("unchanged").unwrap(), commit);
    candidate.export_branch().unwrap();
    let receipt = verify(home, &candidate, &commit, root);
    assert!(receipt.success);
    let judgment = completion(home, worker, &candidate, &receipt, root, state);
    let admin = fs::read_dir(worker.git_dir.join("worktrees"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let pointer = admin.join("commondir");
    let original = fs::read(&pointer).unwrap();
    fs::write(&pointer, source.join(".git").to_string_lossy().as_bytes()).unwrap();
    let rejected = worker.checkpoint_bundle(&executor, &root.join("host-escape.bundle"));
    fs::write(pointer, original).unwrap();
    assert!(
        rejected.is_err(),
        "worker metadata redirected checkpoint into the host repo"
    );
    assert!(!source.join("marker.txt").exists());
    let head = std::process::Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        worker.base_commit
    );
    assert_eq!(candidate.checkpoint("still unchanged").unwrap(), commit);
    json!({"jevCompletion":judgment,"sandboxCheckpoint":true,"trustedBundleImport":true,"verification":receipt,"hostMetadataRedirectRefused":true,"sourceHeadUnchanged":true,"candidateCommitUnchanged":true})
}

fn verify(home: &Path, candidate: &Worktree, commit: &str, root: &Path) -> verification::Receipt {
    let tree = candidate
        .verification_snapshot(&root.join("candidate/verify"), commit)
        .unwrap();
    let uid = kah_run::runtime::service_uid().unwrap();
    access::share_tree(&tree.path, uid, true).unwrap();
    access::share_tree(&tree.git_dir, uid, false).unwrap();
    let project = Project {
        domain: Domain::Research,
        root: tree.path.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let mut executor = kah_cli::native_setup::executor(
        home,
        &project,
        LaunchOptions::default(),
        Some(&tree.git_dir),
    )
    .unwrap()
    .with_read_only_mounts(&[
        (tree.git_dir.clone(), tree.git_dir.clone()),
        (tree.path.join(".git"), tree.path.join(".git")),
    ])
    .unwrap();
    executor.use_nobody();
    let receipt = verification::execute(
        &executor,
        "test \"$(cat marker.txt)\" = PANE_OK && test \"$(cat bash-proof.txt)\" = BASH_OK",
        commit,
        Duration::from_secs(30),
        &root.join("verification.log"),
    )
    .unwrap();
    assert_eq!(tree.checkpoint("verification unchanged").unwrap(), commit);
    tree.remove_verification().unwrap();
    receipt
}

fn completion(
    home: &Path,
    worker: &Worktree,
    candidate: &Worktree,
    receipt: &verification::Receipt,
    root: &Path,
    state: &kah_observer::session::State,
) -> Value {
    use kah_codex::{control::Control, overlay};
    let inspection = root.join("inspection");
    overlay::prepare(
        &home.join(".codex"),
        &inspection,
        &overlay::Policy {
            restricted: true,
            online: false,
            root: &worker.path,
            cwd: &worker.path,
        },
    )
    .unwrap();
    let mut client = Control::connect(
        &state.binding.native,
        &inspection,
        &worker.path,
        Duration::from_secs(10),
    )
    .unwrap();
    let source =
        kah_codex::source::observe(&mut client, &worker.path, &state.binding.session).unwrap();
    let projection = kah_observer::engine::projection(state).unwrap().unwrap();
    let settings = kah_cli::unattended_observer::Settings {
        test: receipt.command.clone(),
        observer: state.binding.clone(),
        allow_hosts: vec![],
    };
    let result = kah_cli::unattended_observer::complete_candidate(
        home,
        &settings,
        &projection,
        &source.actions,
        candidate,
        receipt,
        false,
    )
    .unwrap();
    assert!(
        result.complete,
        "Jev did not confirm the candidate: {}",
        serde_json::to_string(&result.judgments).unwrap()
    );
    json!({"complete":result.complete,"judgments":result.judgments})
}
