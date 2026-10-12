//! Unsettled authority stays native; the last reviewed Git boundary stays intact.
use kah_cli::observer_review::{option1, state::State};
use kah_core::run_store::RunStore;
use kah_observer::{
    intake::Input,
    session::{self, Binding},
};
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn unsettled_set_uses_the_native_reviewed_baseline_without_resolving_authority() {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let root = std::env::temp_dir().join(format!("direct-baseline-{}", std::process::id()));
    let project = root.join("repos/mine/fixture");
    let (baseline, head) = seed_project(&project);
    let worker = format!("direct-baseline-{}", std::process::id());
    let mut primary =
        session::Store::open_wait(&RunStore::default_state_root().unwrap(), &worker).unwrap();
    let binding = Binding {
        session: worker.clone(),
        project: project.clone(),
        codex_home: root.join(".codex"),
        native: env!("CARGO_BIN_EXE_kah").into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 120,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    primary.save(&session::State::new(binding)).unwrap();
    let pending: Input = serde_json::from_value(json!({"source":{"provider":"codex","session":worker,"record":"pending-authority","role":"user","content_digest":"sha256:fixture"},"text":"Unsettled original source","decision":{"kind":"gap","reason":"fixture uncertainty"}})).unwrap();
    primary.add_source(&pending).unwrap();
    drop(primary);
    assert!(
        option1::current_set::resolve(&worker, "fixture-observer", &head)
            .unwrap()
            .is_none()
    );
    let endpoint = serde_json::from_value(
        json!({"provider":"codex","session":worker,"project":project,"pane":null,"socket":null}),
    )
    .unwrap();
    let state = State::new(endpoint, &root).unwrap();
    fs::create_dir_all(state.observer.workspace.join("bin")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_kah"),
        state.observer.workspace.join("bin/kah"),
    )
    .unwrap();
    let private = root.join("review");
    kah_cli::unattended_report::save(
        &private,
        "option1-baseline.json",
        &json!({"head":baseline,"reviewed":"native"}),
    )
    .unwrap();
    let context = option1::direct::context(&root, &state, &private, &head).unwrap();
    assert_eq!(context["baseline"], baseline);
    assert_eq!(context["touched_files"].as_array().unwrap().len(), 1);
    assert_eq!(context["touched_files"][0]["path"], "changed.txt");
    let mut primary =
        session::Store::open_wait(&RunStore::default_state_root().unwrap(), &worker).unwrap();
    assert!(!primary.sources().unwrap()[0].dismissed && !primary.sources().unwrap()[0].promoted);
    primary.save(&primary.load().unwrap().unwrap()).unwrap();
    drop(primary);
    fs::remove_dir_all(root).unwrap();
}

fn seed_project(project: &Path) -> (String, String) {
    fs::create_dir_all(project).unwrap();
    git(project, &["init", "-q"]);
    fs::write(project.join("keep.txt"), "untouched\n").unwrap();
    fs::write(project.join("changed.txt"), "before\n").unwrap();
    git(project, &["add", "."]);
    git(project, &["commit", "-qm", "baseline"]);
    let baseline = git(project, &["rev-parse", "HEAD"]);
    fs::write(project.join("changed.txt"), "after\n").unwrap();
    git(project, &["add", "."]);
    git(project, &["commit", "-qm", "change"]);
    let head = git(project, &["rev-parse", "HEAD"]);
    (baseline, head)
}
