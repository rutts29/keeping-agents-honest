//! The session-start HEAD is the accepted baseline; only later commits are judged.
use kah_cli::observer_review::{
    delivered, option1, queue,
    state::{State, Store},
    tags,
};
use kah_core::run_store::RunStore;
use kah_observer::session::{self, Binding};
use serde_json::{Value, json};
use std::{cell::RefCell, fs, path::Path, process::Command};

const RULE: &str = "# Rules\n\nNever add wrappers around vendored code.\n";
const WRAPPER: &str =
    "from vendor.legacy import render\n\ndef render_report(x):\n    return render(x)\n";

fn git(root: &Path, args: &[&str], date: Option<&str>) -> String {
    let mut command = Command::new("git");
    if let Some(date) = date {
        command
            .env("GIT_COMMITTER_DATE", date)
            .env("GIT_AUTHOR_DATE", date);
    }
    let output = command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@invalid",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path, files: &[(&str, &str)], date: Option<&str>) -> String {
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    git(root, &["add", "."], None);
    git(root, &["commit", "-qm", "fixture"], date);
    git(root, &["rev-parse", "HEAD"], None)
}
fn refs(root: &Path) -> Vec<String> {
    let format = "--format=%(refname) %(objectname)";
    let args = ["for-each-ref", "--include-root-refs", format, "HEAD"];
    git(root, &args, None).lines().map(str::to_owned).collect()
}
fn committed(root: &Path, head: &str) -> u64 {
    git(root, &["show", "-s", "--format=%ct", head], None)
        .parse()
        .unwrap()
}
/// One sample: returns every HEAD the sample sent to commit review.
fn sample(review: &Path, project: &Path) -> Vec<String> {
    sample_started(review, project, None)
}
/// One sample whose fallback session start is `started_ms`.
fn sample_started(review: &Path, project: &Path, started_ms: Option<u64>) -> Vec<String> {
    let reviewed = RefCell::new(vec![]);
    let enqueue = |head: &str| {
        reviewed.borrow_mut().push(head.to_owned());
        Ok(())
    };
    let start = |head: &str| Ok((Some(committed(project, head) * 1000), started_ms));
    tags::sample_head(review, &refs(project), start, enqueue).unwrap();
    reviewed.into_inner()
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let project = root.join("repos/mine/fixture");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q"], None);
    let review = root.join("review");
    fs::create_dir_all(review.join("sources")).unwrap();
    (root, project, review)
}
fn state(root: &Path, project: &Path, worker: &str) -> State {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    let mut primary =
        session::Store::open_wait(&RunStore::default_state_root().unwrap(), worker).unwrap();
    let binding = Binding {
        session: worker.into(),
        project: project.into(),
        codex_home: root.join(".codex"),
        native: env!("CARGO_BIN_EXE_kah").into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 120,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    primary.save(&session::State::new(binding)).unwrap();
    drop(primary);
    let endpoint = serde_json::from_value(
        json!({"provider":"codex","session":worker,"project":project,"pane":null,"socket":null}),
    )
    .unwrap();
    let state = State::new(endpoint, root).unwrap();
    fs::create_dir_all(state.observer.workspace.join("bin")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_kah"),
        state.observer.workspace.join("bin/kah"),
    )
    .unwrap();
    state
}

#[test]
fn seed_violation_is_the_accepted_baseline_and_a_later_agent_violation_is_reviewed() {
    let (root, project, review) = fixture("session-start-baseline");
    let seed = commit(
        &project,
        &[
            ("AGENTS.md", RULE),
            ("vendor/legacy.py", "def render(x):\n    return x\n"),
            ("src/report.py", WRAPPER),
        ],
        Some("2026-01-01T00:00:00Z"),
    );
    capture_start(&review, &project);
    // (a) The session-start HEAD breaks AGENTS.md, but it is not agent work.
    assert!(sample(&review, &project).is_empty());
    assert_eq!(read(&review.join("git-head.json")), json!(seed));
    let baseline = read(&review.join("option1-baseline.json"));
    assert_eq!(baseline["head"], seed);
    assert_eq!(baseline["reviewed"], "session_start");
    let checkpoint = read(&review.join("option1-checkpoint-baseline.json"));
    assert_eq!(checkpoint["head"], seed);
    assert_eq!(checkpoint["covered_commits"], json!([]));
    assert!(sample(&review, &project).is_empty());
    let state = state(&root, &project, "session-start-baseline-worker");
    let start = option1::direct::context(&root, &state, &review, &seed).unwrap();
    assert_eq!(start["touched_files"], json!([]));
    // (b) An agent commit with the same violation is reviewed as a diff only.
    let agent = commit(&project, &[("src/summary.py", WRAPPER)], None);
    assert_eq!(sample(&review, &project), vec![agent.clone()]);
    let context = option1::direct::context(&root, &state, &review, &agent).unwrap();
    assert_eq!(context["baseline"], seed);
    let touched = context["touched_files"].as_array().unwrap();
    assert_eq!(touched.len(), 1);
    assert_eq!(touched[0]["path"], "src/summary.py");
    assert!(
        touched[0]["patch"]
            .as_str()
            .unwrap()
            .contains("return render(x)")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn first_commit_in_a_new_repository_is_agent_work() {
    let (root, project, review) = fixture("session-start-unborn");
    capture_start(&review, &project);
    assert!(sample(&review, &project).is_empty());
    assert_eq!(read(&review.join("git-head.json")), Value::Null);
    let first = commit(
        &project,
        &[("AGENTS.md", RULE), ("src/report.py", WRAPPER)],
        None,
    );
    assert_eq!(sample(&review, &project), vec![first.clone()]);
    assert!(!review.join("option1-baseline.json").exists());
    assert_eq!(read(&review.join("git-head.json")), json!(first));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_head_committed_after_the_session_start_stays_reviewed() {
    let (root, project, review) = fixture("session-start-race");
    capture_start(&review, &project);
    let agent = commit(
        &project,
        &[("src/report.py", WRAPPER)],
        Some("2099-01-01T00:00:00Z"),
    );
    assert_eq!(sample(&review, &project), vec![agent]);
    assert!(!review.join("option1-baseline.json").exists());
    assert!(!review.join("option1-checkpoint-baseline.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn turn_end_queues_one_checkpoint_over_the_cumulative_change_per_head() {
    let (root, project, _) = fixture("delivered-change");
    let worker = format!("delivered-change-{}", std::process::id());
    let fresh = state(&root, &project, &worker);
    let store = Store::open(&worker).unwrap();
    let review = store.directory.clone();
    let seed = commit(
        &project,
        &[("AGENTS.md", RULE)],
        Some("2026-01-01T00:00:00Z"),
    );
    capture_start(&review, &project);
    assert!(sample(&review, &project).is_empty());
    store.save(&fresh).unwrap();
    drop(store);
    // No deferral recorded: a turn end queues nothing and costs nothing.
    assert!(!delivered::queue_due(&worker, &root).unwrap());
    let first = commit(&project, &[("src/a.py", "fixed()\n")], None);
    let last = commit(&project, &[("docs/CHANGES.md", "- fixed\n")], None);
    let baseline = json!({"head":first,"reviewed":"commit"});
    kah_cli::unattended_report::save(&review, "option1-baseline.json", &baseline).unwrap();
    let marker = json!({"head":first,"rules":["r-doc-changelog"]});
    kah_cli::unattended_report::save(&review, delivered::MARKER, &marker).unwrap();
    assert!(delivered::queue_due(&worker, &root).unwrap());
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].1.trigger["head"], last);
    assert_ne!(jobs[0].1.trigger["review_class"], "commit");
    // A second turn end at the same HEAD queues none.
    assert!(!delivered::queue_due(&worker, &root).unwrap());
    assert_eq!(queue::entries(&worker).unwrap().len(), 1);
    // The checkpoint diff starts at the session baseline, not the last review.
    let state = Store::read(&worker).unwrap().unwrap();
    let whole = option1::direct::context_for(&root, &state, &review, &last, true).unwrap();
    assert_eq!(whole["baseline"], seed);
    assert_eq!(whole["touched_files"].as_array().unwrap().len(), 2);
    let one = option1::direct::context_for(&root, &state, &review, &last, false).unwrap();
    assert_eq!(one["baseline"], first);
    fs::remove_dir_all(review.parent().unwrap()).unwrap();
    fs::remove_dir_all(root).unwrap();
}

fn capture_start(review: &Path, project: &Path) {
    use kah_cli::observer_review::source_store;
    let directory = review.join("artifact-baseline");
    let _lock = Store::delivery_at(&directory, true).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kah"))
        .arg("__review-artifact-baseline")
        .current_dir(project)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut baseline: Value = serde_json::from_slice(&output.stdout).unwrap();
    baseline["initial"] = true.into();
    source_store::save(&directory, "baseline.json", &baseline).unwrap();
}

#[test]
fn same_second_post_start_commit_is_not_the_captured_baseline() {
    let (root, project, review) = fixture("same-second-start");
    let seed = commit(
        &project,
        &[("seed.txt", "seed")],
        Some("1970-01-01T00:16:40Z"),
    );
    capture_start(&review, &project);
    let agent = commit(
        &project,
        &[("agent.txt", "work")],
        Some("1970-01-01T00:16:40Z"),
    );
    assert_eq!(committed(&project, &seed), 1000);
    assert_eq!(committed(&project, &agent), 1000);
    assert_eq!(sample(&review, &project), vec![agent]);
    for name in ["option1-baseline.json", "option1-checkpoint-baseline.json"] {
        let stored: Value =
            kah_cli::observer_review::source_store::read(&review.join(name)).unwrap();
        assert_eq!(stored["head"], seed);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_captured_head_is_accepted_regardless_of_its_commit_timestamp() {
    let (root, project, review) = fixture("captured-start");
    let seed = commit(
        &project,
        &[("seed.txt", "seed")],
        Some("2099-01-01T00:00:00Z"),
    );
    capture_start(&review, &project);
    assert!(sample(&review, &project).is_empty());
    for name in ["option1-baseline.json", "option1-checkpoint-baseline.json"] {
        let stored: Value =
            kah_cli::observer_review::source_store::read(&review.join(name)).unwrap();
        assert_eq!(stored["head"], seed);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unborn_start_does_not_accept_a_commit_before_the_first_sample() {
    let (root, project, review) = fixture("unborn-late-sample");
    capture_start(&review, &project);
    let agent = commit(
        &project,
        &[("agent.txt", "work")],
        Some("1970-01-01T00:16:40Z"),
    );
    assert_eq!(sample(&review, &project), vec![agent]);
    assert!(!review.join("option1-baseline.json").exists());
    assert!(!review.join("option1-checkpoint-baseline.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn without_a_start_capture_a_head_committed_before_the_start_is_accepted() {
    // A resumed session (or a failed capture) has no initial HEAD record.
    let (root, project, review) = fixture("uncaptured-start");
    let seed = commit(
        &project,
        &[("seed.txt", "seed")],
        Some("2026-01-01T00:00:00Z"),
    );
    let started = committed(&project, &seed) * 1000 + 500;
    assert!(sample_started(&review, &project, Some(started)).is_empty());
    for name in ["option1-baseline.json", "option1-checkpoint-baseline.json"] {
        assert_eq!(read(&review.join(name))["head"], seed);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn without_a_start_capture_a_head_committed_after_the_start_stays_reviewed() {
    let (root, project, review) = fixture("uncaptured-late");
    let agent = commit(
        &project,
        &[("agent.txt", "work")],
        Some("2026-01-01T00:00:01Z"),
    );
    let started = committed(&project, &agent) * 1000 - 1;
    assert_eq!(
        sample_started(&review, &project, Some(started)),
        vec![agent]
    );
    assert!(!review.join("option1-baseline.json").exists());
    assert!(!review.join("option1-checkpoint-baseline.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_capture_whose_head_read_failed_uses_the_timestamp_fallback() {
    use kah_cli::observer_review::source_store;
    let (root, project, review) = fixture("headless-capture");
    let seed = commit(
        &project,
        &[("seed.txt", "seed")],
        Some("2026-01-01T00:00:00Z"),
    );
    // The inventory was captured, but its HEAD read failed: no `head` field.
    let capture = json!({"initial":true,"blobs":{},"unsupported":{},"captured_ms":1});
    source_store::save(&review.join("artifact-baseline"), "baseline.json", &capture).unwrap();
    let started = committed(&project, &seed) * 1000 + 500;
    assert!(sample_started(&review, &project, Some(started)).is_empty());
    assert_eq!(read(&review.join("option1-baseline.json"))["head"], seed);
    fs::remove_dir_all(root).unwrap();
}

/// HEAD and the tags, read as tags::run reads them.
fn refs_with_tags(root: &Path) -> Vec<String> {
    let format = "--format=%(refname) %(objectname)";
    let args = [
        "for-each-ref",
        "--include-root-refs",
        format,
        "refs/tags",
        "HEAD",
    ];
    git(root, &args, None).lines().map(str::to_owned).collect()
}
/// One sample of HEAD and tags: the HEADs and the tag names sent to review.
fn sample_refs(
    review: &Path,
    project: &Path,
    started_ms: Option<u64>,
) -> (Vec<String>, Vec<String>) {
    let refs = refs_with_tags(project);
    let start = |head: &str| Ok((Some(committed(project, head) * 1000), started_ms));
    let mut heads = vec![];
    let accepted = tags::sample_head(review, &refs, start, |head| {
        heads.push(head.to_owned());
        Ok(())
    })
    .unwrap();
    let mut names = vec![];
    tags::sample_tags(review, &refs, accepted, |added, _| {
        names = added
            .iter()
            .map(|t| t.split(' ').next().unwrap().to_owned())
            .collect();
        Ok(())
    })
    .unwrap();
    (heads, names)
}
fn seeded_with_tag(name: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let (root, project, review) = fixture(name);
    commit(
        &project,
        &[("seed.txt", "seed")],
        Some("2026-01-01T00:00:00Z"),
    );
    git(&project, &["tag", "v0.1"], None);
    (root, project, review)
}

#[test]
fn tags_at_the_session_start_are_the_baseline_and_a_later_tag_is_reviewed() {
    // Pilot #41: the first sample reviewed every existing tag, so a checkpoint
    // ran on the untouched start commit before the worker changed anything.
    let (root, project, review) = seeded_with_tag("session-start-tags");
    capture_start(&review, &project);
    assert_eq!(sample_refs(&review, &project, None), (vec![], vec![]));
    git(&project, &["tag", "v0.2"], None);
    let later = sample_refs(&review, &project, None);
    assert_eq!(later, (vec![], vec!["refs/tags/v0.2".into()]));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tags_made_with_a_commit_before_the_first_sample_are_reviewed() {
    // 0673c8b: a late first sample must not hide the agent's first tag. The
    // start HEAD is not accepted, so the sample cannot tell old tags from new.
    let (root, project, review) = seeded_with_tag("session-start-tag-race");
    capture_start(&review, &project);
    let agent = commit(&project, &[("agent.txt", "work")], None);
    git(&project, &["tag", "phase-1"], None);
    let (heads, names) = sample_refs(&review, &project, None);
    assert_eq!(heads, vec![agent]);
    assert_eq!(names, ["refs/tags/phase-1", "refs/tags/v0.1"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_tag_on_the_untouched_start_head_before_the_first_sample_is_not_reviewed() {
    // No worker change exists yet, so there is nothing for a checkpoint to judge.
    let (root, project, review) = seeded_with_tag("session-start-tag-unchanged");
    capture_start(&review, &project);
    git(&project, &["tag", "phase-0"], None);
    assert_eq!(sample_refs(&review, &project, None), (vec![], vec![]));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn without_a_start_capture_tags_at_an_accepted_head_are_the_baseline() {
    let (root, project, review) = seeded_with_tag("uncaptured-start-tags");
    let seed = git(&project, &["rev-parse", "HEAD"], None);
    let started = committed(&project, &seed) * 1000 + 500;
    assert_eq!(
        sample_refs(&review, &project, Some(started)),
        (vec![], vec![])
    );
    fs::remove_dir_all(root).unwrap();
}
