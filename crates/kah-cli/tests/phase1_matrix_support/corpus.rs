//! A generated corpus for the live matrix journeys: a small repository, a
//! synthetic user ledger, two measured commits, the atomization input and
//! labelled synthetic flags. No operator corpus or earlier session is read.
use super::*;
use kah_cli::commit_measure::{self, corpus, historical::Snapshot};
use kah_core::{
    intent_history::{
        Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
    },
    projection::{ActiveProjection, ProjectionConfig},
};

const GOAL: &str = "Build a small request library that keeps user data exactly as given.";
/// (case id, file, added source, rule id, rule text): one touched rule each.
const CASES: [(&str, &str, &str, &str, &str); 2] = [
    (
        "case-limits",
        "src/limits.rs",
        "pub const MAX_ITEMS: usize = 100;\n",
        "user-no-fixed-limits",
        "Never add a fixed upper limit on the number of items a request may carry.",
    ),
    (
        "case-names",
        "src/names.rs",
        "pub fn label(name: &str) -> String {\n    name.to_uppercase()\n}\n",
        "user-keep-names",
        "Keep user-supplied names exactly as written; never change their case.",
    ),
];

/// Writes the corpus under `root` and returns its directory.
pub fn generate(root: &Path) -> PathBuf {
    let repo = root.join("corpus-repo");
    let output = root.join("corpus");
    fs::create_dir_all(repo.join("src")).unwrap();
    kah_core::run_store::ensure_private_directory(&output).unwrap();
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("README.md"), "# Requests\n").unwrap();
    commit(&repo, "start");
    let mut cases = vec![];
    for (id, file, text, rule, words) in CASES {
        fs::write(repo.join(file), text).unwrap();
        let head = commit(&repo, id);
        let bundle =
            corpus::real_commit_with_snapshot(&repo, &head, snapshot(&repo, &head, rule, words))
                .unwrap();
        let directory = output.join(id);
        commit_measure::count(&bundle, &directory.join("all_changed_files")).unwrap();
        cases.push(json!({"id":id,"head":head,"directory":directory}));
    }
    save(
        &output.join("report.json"),
        &json!({"cases":cases,"synthetic":true}),
    );
    let items = json!([
        {"id":"user-no-fixed-limits","kind":"constraint","text":CASES[0].4},
        {"id":"user-review-notes","kind":"invariant",
         "text":"Every reviewed change keeps a short note on why it was made, written after the review meeting."}
    ]);
    save(&output.join("intent-only.json"), &items);
    output
}
/// One labelled flag per case, bound to that case's measured request: the
/// flag names the request's own question for the rule. These exercise the
/// confirmation transport; they are not Jev answers.
pub fn flags(corpus: &Path) -> PathBuf {
    let output = corpus.join("jev-output");
    kah_core::run_store::ensure_private_directory(&output).unwrap();
    let mut records = vec![];
    for (index, (id, file, _, rule, words)) in CASES.into_iter().enumerate() {
        let request = corpus.join(id).join("all_changed_files/literal.json");
        let bytes = fs::read(&request).unwrap();
        let question = question(&serde_json::from_slice(&bytes).unwrap(), rule);
        let job = output.join(format!("{id}-job.json"));
        save(
            &job,
            &json!({"case_id":id,"file":file,"rule":rule,"rule_text":words,"request":request,
                "question":question,"request_digest":kah_observer::commit_check::digest(&bytes)}),
        );
        records.push(json!({"index":index,"job":{"case_id":id,"path":job},
            "flags":[{"question":question,"label":"synthetic"}]}));
    }
    save(
        &output.join("complete.json"),
        &json!({"records":records,"source":"labelled synthetic flags; no Jev call"}),
    );
    output
}
/// The request's question key for `rule`: `rule.<check key>` of the check
/// whose requirement is that rule.
fn question(request: &Value, rule: &str) -> String {
    let checks = request["state"]["checks"].as_object().unwrap();
    let (key, _) = checks
        .iter()
        .find(|(_, check)| check["requirement"] == rule)
        .unwrap_or_else(|| panic!("no literal check for {rule}"));
    let question = format!("rule.{key}");
    assert!(request["questions"].get(&question).is_some(), "{question}");
    question
}
fn snapshot(repo: &Path, head: &str, rule: &str, words: &str) -> Snapshot {
    let source = |record: &str, text: &str| {
        SourceRef::new(
            SourceProvider::Bundle,
            "corpus-session",
            record,
            SourceRole::User,
            kah_observer::commit_check::digest(text.as_bytes()),
        )
    };
    let mut history = History::new(GoalRevision::new(1, GOAL, vec![source("goal", GOAL)])).unwrap();
    let item = IntentItem::new(
        rule,
        IntentKind::Constraint,
        words,
        vec![source(rule, words)],
    );
    history
        .append(Event::RequirementAdded {
            item,
            approval: None,
        })
        .unwrap();
    let projection = ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap();
    let at: u64 = git(repo, &["show", "-s", "--format=%ct", head])
        .parse()
        .unwrap();
    Snapshot {
        audit: json!({"commit_at_ms":at * 1000,"ledger_revision":projection.revision}),
        projection,
    }
}
fn commit(repo: &Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}
fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=Sample"])
        .args(["-c", "user.email=sample@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
