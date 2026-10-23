//! Commit measurement on a generated repository: a small Git history and a
//! synthetic user ledger. Unconditional prohibitions need no component link.
//! No live Jev and no retained corpus.
use kah_cli::commit_measure::{self, corpus, historical::Snapshot};
use kah_core::{
    intent_history::{
        Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
    },
    projection::{ActiveProjection, ProjectionConfig},
};
use kah_observer::commit_check::{CodeQuote, FindingKind, Row, Status};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const CONSUME: &str = "src/consume.rs";
const CAP: &str = "Duration::from_secs(110)";
const BASE: &str = "use std::time::Duration;\n\npub fn wait_for_report(poll: impl Fn() -> bool) {\n    while !poll() {\n        std::thread::sleep(Duration::from_millis(200));\n    }\n}\n";
const DEADLINE: &str = "\npub fn wait_with_deadline(poll: impl Fn() -> bool) -> bool {\n    let deadline = std::time::Instant::now() + Duration::from_secs(110);\n    while !poll() {\n        if std::time::Instant::now() > deadline {\n            return false;\n        }\n        std::thread::sleep(Duration::from_millis(200));\n    }\n    true\n}\n";

/// A temporary repository. `positive` adds a fixed review deadline; `archive`
/// copies a file byte for byte and edits another one. All commits fall inside
/// the corpus sampling window, so the control sample can read them.
struct Repo {
    root: PathBuf,
    base: String,
    positive: String,
    archive: String,
    labels: Value,
    minute: std::cell::Cell<u32>,
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Repo {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("commit-measure-{}", kah_cli::relay_store::nonce()));
        fs::create_dir_all(root.join("src")).unwrap();
        let mut repo = Repo {
            root,
            base: String::new(),
            positive: String::new(),
            archive: String::new(),
            labels: Value::Null,
            minute: std::cell::Cell::new(0),
        };
        repo.git(&["init", "-q"]);
        repo.write("README.md", "# Sample\n");
        repo.write(CONSUME, BASE);
        repo.write("src/limits.rs", "pub const BATCH: usize = 8;\n");
        repo.base = repo.commit("start");
        repo.write(CONSUME, &format!("{BASE}{DEADLINE}"));
        repo.write(
            "src/limits.rs",
            "pub const BATCH: usize = 16;\npub const RETRIES: usize = 3;\n",
        );
        repo.positive = repo.commit("add a review deadline");
        fs::create_dir_all(repo.root.join("archive")).unwrap();
        repo.write("archive/consume.rs", &format!("{BASE}{DEADLINE}"));
        repo.write("README.md", "# Sample\n\nThe archive keeps the old wait.\n");
        repo.archive = repo.commit("archive the wait");
        repo.labels = repo.history_with_controls();
        repo
    }
    /// Five labelled positive/fix pairs (the deadline commit is the fourth)
    /// and twenty ordinary control commits.
    fn history_with_controls(&self) -> Value {
        let mut positives = vec![];
        for n in 0..5 {
            let positive = if n == 3 {
                self.positive.clone()
            } else {
                self.write("notes.txt", &format!("positive {n}\n"));
                self.commit(&format!("positive {n}"))
            };
            self.write("notes.txt", &format!("fix {n}\n"));
            let fix = self.commit(&format!("fix {n}"));
            positives.push(json!({"commit":positive,"fixed_by":fix}));
        }
        positives[3]["evidence"] = json!({"file":CONSUME,"added_line":CAP});
        for n in 0..20 {
            self.write("notes.txt", &format!("control {n}\n"));
            self.commit(&format!("control {n}"));
        }
        json!({"positives":positives,"requirements":{
            "no_caps":{"text":"Scoring label: no fixed caps."},
            "jev_literal_questions":{"text":"Scoring label: literal questions."}}})
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.root.join(path), text).unwrap();
    }
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        let minute = self.minute.get();
        self.minute.set(minute + 1);
        let date = format!("2026-10-03T{:02}:{:02}:00Z", 10 + minute / 60, minute % 60);
        let output = self
            .command(&["commit", "-q", "--allow-empty", "-m", message])
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        self.git(&["rev-parse", "HEAD"])
    }
    fn git(&self, args: &[&str]) -> String {
        let output = self.command(args).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=Sample"])
            .args(["-c", "user.email=sample@example.invalid", "-c"])
            .args(["commit.gpgsign=false"])
            .args(args)
            .current_dir(&self.root);
        command
    }
    /// The user ledger at the deadline commit, bound to its commit time.
    fn snapshot(&self) -> Snapshot {
        let source = |record: &str, text: &str| {
            SourceRef::new(
                SourceProvider::Bundle,
                "sample-session",
                record,
                SourceRole::User,
                kah_observer::commit_check::digest(text.as_bytes()),
            )
        };
        let goal = "Keep the review loop simple and let every review take as long as it needs.";
        let mut history =
            History::new(GoalRevision::new(1, goal, vec![source("goal", goal)])).unwrap();
        for (id, kind, text) in [
            (
                "user-no-deadline",
                IntentKind::Constraint,
                "Never add a fixed deadline to a review wait.",
            ),
            (
                "user-reports-kept",
                IntentKind::Invariant,
                "Every finished review report stays on disk.",
            ),
            (
                "user-plain-names",
                IntentKind::Outcome,
                "Give new functions plain names.",
            ),
            (
                "user-remove-deadline",
                IntentKind::Constraint,
                "Remove the fixed review deadline now; review waits stay unbounded.",
            ),
        ] {
            let item = IntentItem::new(id, kind, text, vec![source(id, text)]);
            let event = Event::RequirementAdded {
                item,
                approval: None,
            };
            history.append(event).unwrap();
        }
        let projection =
            ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap();
        let at: u64 = self
            .git(&["show", "-s", "--format=%ct", &self.positive])
            .parse()
            .unwrap();
        Snapshot {
            audit: json!({"commit_at_ms":at * 1000,"ledger_revision":projection.revision}),
            projection,
        }
    }
    fn bundle(&self) -> commit_measure::Bundle {
        corpus::real_commit_with_snapshot(&self.root, &self.positive, self.snapshot()).unwrap()
    }
}
fn root(repo: &Repo) -> &Path {
    &repo.root
}

#[test]
fn historical_constraints_are_prepared_without_a_component_link_or_any_client() {
    let repo = Repo::new();
    let mut bundle = repo.bundle();
    let expected = bundle
        .input
        .rules
        .iter()
        .filter(|(_, r)| r.unconditional_prohibition)
        .map(|(id, _)| id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!expected.is_empty());
    assert!(!expected.contains("user-plain-names"));
    bundle.input.impacted.clear();
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert_eq!(plan.touched, expected);
    let body = plan.literal.body().unwrap();
    assert!(body.contains("current_permissions"));
    assert!(!body.contains("jevliteralquestions"));
    assert!(bundle.quality.is_empty());
    let directory = repo.root.join("measure");
    let report = commit_measure::count(&bundle, &directory).unwrap();
    assert_eq!(report["actual_jev_calls"], 0);
    assert_eq!(report["actual_jev_cost_usd"], 0.0);
    assert_eq!(report["quality_judgment"], "not_run");
    assert_eq!(
        fs::read_to_string(directory.join("literal.json")).unwrap(),
        body
    );
}
#[test]
fn missing_historical_intent_is_reported_instead_of_invented_from_later_labels() {
    let repo = Repo::new();
    let error = corpus::real_commit(root(&repo), &repo.archive)
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .starts_with("MEASURE_HISTORICAL_INTENT_MISSING")
    );
}
#[test]
fn actual_added_cap_quote_is_verified_and_safe_choice_order_is_preserved() {
    let repo = Repo::new();
    let mut bundle = repo.bundle();
    let rule_id = bundle
        .input
        .intent
        .items
        .iter()
        .find(|(_, a)| a.text.contains("Remove the fixed review deadline now"))
        .unwrap()
        .0
        .clone();
    bundle.input.rules.get_mut(&rule_id).unwrap().prohibition = true;
    let file = &bundle.input.files[CONSUME];
    let (index, text) = file
        .text
        .split_inclusive('\n')
        .enumerate()
        .find(|(_, line)| line.contains(CAP))
        .unwrap();
    bundle.input.matrix = vec![Row {
        requirement: rule_id.clone(),
        finding: "The new wait sets a 110 second deadline.".into(),
        status: Status::Violated,
        kind: FindingKind::Behavior,
        quotes: vec![CodeQuote {
            file: CONSUME.into(),
            first_line: index + 1,
            last_line: index + 1,
            text: text.into(),
            digest: file.digest.clone(),
        }],
    }];
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert!(plan.correction_signals.is_empty());
    assert_eq!(plan.citations.questions.len(), 1);
    let body = plan.citations.body().unwrap();
    assert!(body.find("\"says_nothing\":").unwrap() < body.find("\"contradicts\":").unwrap());
    assert!(body.find("\"contradicts\":").unwrap() < body.find("\"supports\":").unwrap());
    let mut absence = bundle.input.matrix[0].clone();
    absence.status = Status::Satisfied;
    absence.kind = FindingKind::NotViolated;
    absence.finding = "No fixed deadline added.".into();
    absence.quotes.clear();
    bundle.input.matrix = vec![absence];
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert!(plan.citations.questions.is_empty());
    assert_eq!(plan.absence_rows, [rule_id].into());
}
#[test]
fn labelled_control_sample_is_deterministic_and_excludes_every_positive_and_fix() {
    let repo = Repo::new();
    let labels = &repo.labels;
    let first = corpus::controls(root(&repo), labels).unwrap();
    assert_eq!(first, corpus::controls(root(&repo), labels).unwrap());
    assert!(first.len() >= labels["positives"].as_array().unwrap().len() * 4);
    for control in &first {
        for positive in labels["positives"].as_array().unwrap() {
            assert!(!control.starts_with(positive["commit"].as_str().unwrap()));
            assert!(!control.starts_with(positive["fixed_by"].as_str().unwrap()));
        }
    }
    assert_eq!(json!(first).as_array().unwrap().len(), 20);
}

#[test]
fn scoring_labels_cannot_change_the_same_commits_authority_or_questions() {
    let repo = Repo::new();
    let original = repo.labels.clone();
    let mut changed = original.clone();
    changed["requirements"]["no_caps"]["text"] = json!("Injected scoring-only instruction");
    changed["requirements"]["jev_literal_questions"]["text"] = json!("Another injected label");
    let from_scoring_case = |scoring: &Value| {
        let sha = scoring["positives"][3]["commit"].as_str().unwrap();
        assert_eq!(sha, repo.positive);
        repo.bundle()
    };
    let before = from_scoring_case(&original);
    let after = from_scoring_case(&changed);
    assert!(before.quality.is_empty());
    assert!(!before.input.rules.contains_key("no_caps"));
    assert!(!before.input.rules.contains_key("jev_literal_questions"));
    assert_eq!(
        commit_measure::prepare(&before)
            .unwrap()
            .literal
            .body()
            .unwrap(),
        commit_measure::prepare(&after)
            .unwrap()
            .literal
            .body()
            .unwrap()
    );
}

#[test]
fn scoring_matches_labelled_code_evidence_not_injected_authority_aliases() {
    let repo = Repo::new();
    let label = repo.labels["positives"][3].clone();
    let bundle = repo.bundle();
    let rule_id = bundle
        .input
        .rules
        .keys()
        .find(|id| id.starts_with("user-"))
        .unwrap();
    let source = bundle.input.files[CONSUME]
        .text
        .lines()
        .find(|line| line.contains(CAP))
        .unwrap();
    let case = json!({"id":"sample-positive","kind":"real_positive","label":label,
        "matrix_rows":[{"requirement":rule_id,"status":"violated",
        "quotes":[{"file":CONSUME,"text":source}]}]});
    let score = commit_measure::replay::summarize(std::slice::from_ref(&case));
    assert_eq!(score["matrix_misses"], 0);
    let mut wrong = case;
    wrong["matrix_rows"][0]["quotes"][0]["text"] = json!("An unrelated line");
    assert_eq!(
        commit_measure::replay::summarize(&[wrong])["matrix_misses"],
        1
    );
}

#[test]
fn request_shares_changed_code_once_and_uses_stored_literal_typing() {
    let repo = Repo::new();
    let bundle = repo.bundle();
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert_eq!(plan.literal.state["diff_hunks"], json!(bundle.input.hunks));
    assert!(
        plan.literal.state["checks"]
            .as_object()
            .unwrap()
            .values()
            .all(|rule| rule.get("diff_hunks").is_none())
    );
    let literal_ids: std::collections::BTreeSet<_> = plan.literal.state["checks"]
        .as_object()
        .unwrap()
        .values()
        .map(|rule| rule["requirement"].as_str().unwrap().to_owned())
        .collect();
    assert!(!literal_ids.contains("goal"));
    let projection = &bundle.historical.as_ref().unwrap().projection;
    let expected: std::collections::BTreeSet<_> = projection
        .preserve
        .iter()
        .chain(&projection.frontier)
        .chain(&projection.validation)
        .filter(|item| matches!(item.kind, IntentKind::Constraint | IntentKind::Invariant))
        .map(|item| item.id.to_string())
        .collect();
    assert_eq!(literal_ids, expected);
    assert!(plan.touched.contains("goal"));
}

#[test]
fn archive_copy_comparison_retains_full_inputs_and_records_git_identity() {
    let repo = Repo::new();
    let mut bundle = repo.bundle();
    let head = repo.archive.clone();
    let base = repo.git(&["rev-parse", &format!("{head}^1")]);
    let (hunks, files) = commit_measure::collect(root(&repo), &base, &head).unwrap();
    bundle.input.revision = head;
    bundle.input.baseline = Some(base);
    bundle.input.hunks = hunks;
    bundle.input.files = files;
    let original_paths: std::collections::BTreeSet<_> = bundle
        .input
        .hunks
        .values()
        .map(|h| h.file.clone())
        .collect();
    let omissions = commit_measure::comparison::omitted(root(&repo), &bundle.input).unwrap();
    assert!(
        omissions
            .values()
            .any(|v| v["native_status"] == "C100" || v["native_status"] == "R100")
    );
    let compared = commit_measure::comparison::input(&bundle, &omissions).unwrap();
    assert_eq!(
        bundle
            .input
            .hunks
            .values()
            .map(|h| h.file.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        original_paths
    );
    assert!(compared.input.hunks.len() < bundle.input.hunks.len());
    assert!(compared.input.carried["full_input_retained"] == true);
}

#[test]
fn whole_file_decomposition_keeps_every_rule_and_all_source_hunks() {
    let repo = Repo::new();
    let plan = commit_measure::prepare(&repo.bundle()).unwrap();
    let pieces = plan.literal.whole_hunks();
    assert!(
        pieces.len() > 1,
        "two changed files give more than one piece"
    );
    let mut ids = std::collections::BTreeSet::new();
    for piece in pieces {
        assert_eq!(
            serde_json::to_value(&piece.questions).unwrap(),
            serde_json::to_value(&plan.literal.questions).unwrap()
        );
        assert_eq!(
            piece.state["current_permissions"],
            plan.literal.state["current_permissions"]
        );
        let hunks = piece.state["diff_hunks"].as_object().unwrap();
        assert_eq!(hunks.len(), 1);
        for (id, hunk) in hunks {
            assert_eq!(hunk, &plan.literal.state["diff_hunks"][id]);
            ids.insert(id.clone());
        }
    }
    assert_eq!(
        ids,
        plan.literal.state["diff_hunks"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    );
}
