//! $0 transport decisions prove routing mechanics, not new model selection quality.
use super::*;
use crate::observer_review::source_store;
use kah_observer::review_evidence::Document;
use std::{fs, path::PathBuf, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "core.hooksPath=/dev/null", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn facts(root: &Path, case: &str, request: &mut Request, report: &mut Report) {
    let repo = git_fixture(root, case);
    assert_eq!(
        git(&repo, &["rev-parse", "HEAD"]),
        request.head.as_deref().unwrap()
    );
    let doc = request
        .documents
        .iter_mut()
        .find(|d| d.id == "git")
        .unwrap();
    let mut value: Value = serde_json::from_str(&doc.text).unwrap();
    value["tags"] = git(
        &repo,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/tags",
        ],
    )
    .into();
    *doc = Document::new(&doc.id, &doc.kind, &doc.uri, &value.to_string());
    // The negative report claims success, but cites the genuine current Git capture.
    for requirement in &mut report.decision_units.as_mut().unwrap().requirements {
        for citation in &mut requirement.citations {
            if citation.source == "git" {
                citation.digest = doc.digest.clone();
                citation.quote = doc.text.clone();
                citation.occurrence = 0;
            }
        }
    }
    if case == "uncited" {
        report.verification_contracts.as_mut().unwrap()[0]
            .evidence_citations
            .clear();
    }
}

fn judgment() -> Judgment {
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-controller-coverage/retained-evaluation.json"
    ))
    .unwrap();
    let mut result: Judgment = serde_json::from_value(value["judgment"].clone()).unwrap();
    result.backend = "jev".into();
    result.model = "$0-recorded-transport-stub".into();
    result.metrics = Default::default();
    // Synthetic coverage answer exercises routing; the new body is not an old answered receipt.
    let answer = kah_observer::judge::Answer::Choice {
        choice: "supported".into(),
        confidence: 1.0,
        probabilities: [("supported".into(), 1.0)].into_iter().collect(),
    };
    result
        .answers
        .insert("coverage_decision.req-commit-tag".into(), answer);
    result
}

fn round(
    root: &Path,
    request: &Request,
    reason: &str,
) -> super::super::followup::VerificationRound {
    let mut round: super::super::followup::VerificationRound = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-controller-coverage/retained-round.json"
    ))
    .unwrap();
    round.checks.as_mut().unwrap().risks[0].unverified = format!("req-commit-tag: {reason}");
    source_store::save(
        &root.join("gate-followups"),
        &format!("{}.json", round.id),
        &json!(round),
    )
    .unwrap();
    let mut archived = request.clone();
    archived
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    source_store::save(
        &root.join("requests"),
        &format!("{}.json", archived.id),
        &json!(archived),
    )
    .unwrap();
    round
}

fn route(root: &Path, mut request: Request, report: &Report, covered: bool, reason: &str) {
    request
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    let round = round(root, &request, reason);
    let proof = round.checks.unwrap().proof;
    let assessment = if covered {
        json!({"passed":true,"state":"checkpoint_passed","needs_thinker":[],"needs_user":[],"violations":[],"unproven":[]})
    } else {
        json!({"passed":null,"state":"checkpoint_needs_thinker","needs_thinker":["req-commit-tag"],"needs_user":[],"violations":[],"unproven":["req-commit-tag"]})
    };
    let question = super::super::store(
        root,
        &request,
        report,
        &assessment,
        &proof,
        &json!(judgment()),
    )
    .unwrap();
    if covered {
        assert!(question.is_none(), "GOOD asks nothing");
    } else {
        let question = question.expect("one completed round must reach named risk");
        let message = super::super::message(&question).unwrap();
        assert!(message.contains(reason));
        assert!(message.contains(request.head.as_deref().unwrap()));
        assert!(message.contains("or send it back for more evidence?"));
    }
}

#[test]
fn good_controller_facts_ask_nothing_and_all_three_negatives_keep_named_risks() {
    for case in ["good", "missing", "wrong", "uncited"] {
        let root: PathBuf =
            std::env::temp_dir().join(format!("controller-{case}-{}", crate::relay_store::nonce()));
        kah_core::run_store::ensure_save_directory(&root).unwrap();
        kah_core::run_store::ensure_save_directory(&root.join("sources")).unwrap();
        let (mut request, mut report, mut input) = fixture();
        facts(&root, case, &mut request, &mut report);
        let mut questions = Questions::new();
        prepare(&request, &report, &mut input, &mut questions).unwrap();
        assert_eq!(
            questions.contains_key("coverage_decision.req-commit-tag"),
            case == "good"
        );
        let outcome = outcome(&request, &report, &judgment()).unwrap();
        assert_eq!(
            outcome["req-commit-tag"]["covered"],
            case == "good",
            "{case}"
        );
        route(&root, request, &report, case == "good", case);
        println!(
            "CONTROLLER_CASE {case} covered={} Jev_calls=0",
            case == "good"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn git_fixture(root: &Path, case: &str) -> PathBuf {
    let bundle = root.join("source.bundle");
    fs::write(
        &bundle,
        include_bytes!(
            "../../../../docs/gates/evidence/phase3-controller-coverage/retained-source.bundle"
        ),
    )
    .unwrap();
    git(root, &["init", "--bare", "--template=", "repo.git"]);
    let repo = root.join("repo.git");
    git(
        &repo,
        &[
            "-c",
            "fetch.fsckObjects=true",
            "fetch",
            bundle.to_str().unwrap(),
            "HEAD:refs/heads/main",
            "refs/tags/gate-pipeline-ready:refs/tags/gate-pipeline-ready",
        ],
    );
    git(&repo, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    if case == "missing" {
        git(
            &repo,
            &["update-ref", "-d", "refs/tags/gate-pipeline-ready"],
        );
    }
    if case == "wrong" {
        let parent = git(&repo, &["rev-parse", "HEAD~1"]);
        git(
            &repo,
            &["update-ref", "refs/tags/gate-pipeline-ready", &parent],
        );
    }
    repo
}
