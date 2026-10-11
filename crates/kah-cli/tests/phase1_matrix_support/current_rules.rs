//! Native rust-v0.162.1/Herdr0.9.3 launch and hooks reuse the cited Option1 route.
use super::*;
use kah_cli::observer_review::{
    option1::{current_set, journal},
    state::Store,
};
use kah_observer::rule_source::Resolved;
pub fn run(f: &fixture::Fixture) {
    option1::private_key(f);
    option1::git(f, &["init"]);
    fs::write(f.project.join("AGENTS.md"), "Never truncate text input.\n").unwrap();
    let pane = journey::start_worker(
        f,
        "Goal: preserve complete text and object inputs. The private test controller supplies commits. Acknowledge readiness without changing files or claiming a phase or gate complete.",
    );
    let worker = option1::wait_worker(f, &pane);
    let first = wait_set(f, &worker, None);
    let observer = first.observer_session.clone();
    herdr(&[
        "agent",
        "prompt",
        &pane,
        "Constraint: Never remove any object field. Acknowledge this directive without modifying files.",
    ]);
    let second = wait_set(f, &worker, Some(&first.source_digest));
    assert_eq!(second.observer_session, observer);
    assert!(second.rules.iter().any(|r| {
        r.sources
            .iter()
            .any(|s| s.quote.contains("Never remove any object field"))
    }));
    commit_and_check(f, &worker, &observer, first, second);
}
fn commit_and_check(
    f: &fixture::Fixture,
    worker: &str,
    observer: &str,
    first: Resolved,
    second: Resolved,
) {
    fs::write(
        f.project.join("lib.js"),
        "module.exports = value => value;\n",
    )
    .unwrap();
    option1::git(f, &["add", "."]);
    option1::git(f, &["commit", "-m", "preserve complete input"]);
    let head = option1::git(f, &["rev-parse", "HEAD"]);
    option1::sample(f, worker);
    let record = loop {
        if let Some(r) = journal::load(worker, &head).unwrap()
            && (!r.requires_native() || r.native.is_some())
        {
            break r;
        }
        progress(f);
    };
    assert!(
        record.gaps.is_empty(),
        "validated small rules must use the literal screen"
    );
    assert!(!record.answers.is_empty());
    let receipt: Value = kah_cli::observer_review::state::read(
        &Store::directory(worker)
            .unwrap()
            .join(format!("current-rules-commit-{head}.json")),
    )
    .unwrap()
    .unwrap();
    assert_eq!(receipt["set_digest"], second.source_digest);
    assert_eq!(receipt["observer_session"], observer);
    save(
        &f.evidence.join("current-rules-e2e.json"),
        &json!({"worker":worker,"observer":observer,"first":first,"after_intent_change":second,"head":head,"receipt":receipt,"record":record,"versions":versions(),"resources":usage::resources(&f.root)}),
    );
}
fn wait_set(f: &fixture::Fixture, worker: &str, prior: Option<&str>) -> Resolved {
    loop {
        if let Some(s) = Store::read(worker).unwrap()
            && let Some(observer) = s.observer.session
            && let Some(r) = ready_set(worker, &observer)
            && prior != Some(r.source_digest.as_str())
            && s.pending.is_none()
        {
            return r;
        }
        progress(f);
    }
}
fn ready_set(worker: &str, observer: &str) -> Option<Resolved> {
    match current_set::resolve(worker, observer, &"a".repeat(40)) {
        Ok(r) => r,
        Err(e) if e.to_string().starts_with("STORE_BUSY") => None,
        Err(e) => panic!("current-rule poll failed: {e}"),
    }
}
fn progress(f: &fixture::Fixture) {
    super::option1_budget::check(f);
    std::thread::sleep(Duration::from_millis(500));
}
