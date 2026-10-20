//! fix-14: a direct Jev call with no size handling stopped the Stop claim
//! check. Every raw Jev entry in crates/*/src is a known site with a note: its
//! size handling, or why it is safe or out of scope. A new site fails here.
use std::{collections::BTreeMap, fs, path::Path};

/// The raw Jev entries: the routed call, the HTTP client and the raw judge.
const ENTRIES: [&str; 5] = [
    "routing::evaluate",
    "HttpClient::from_home",
    "HttpClient::replay",
    "jev::Jev {",
    ".post_raw(",
];

/// (file under crates/, entry, count, note)
const KNOWN: &[(&str, &str, usize, &str)] = &[
    (
        "kah-observer/src/engine/judgment.rs",
        "routing::evaluate",
        1,
        "JEV_CONTEXT_LIMIT goes to context::stage (observer preparation)",
    ),
    (
        "kah-observer/src/evaluation/mod.rs",
        "routing::evaluate",
        1,
        "opt-in offline replay; each error is kept in its case record",
    ),
    (
        "kah-cli/src/completion_replay.rs",
        "routing::evaluate",
        1,
        "manual private replay of a saved request (2 MiB bound); the error is saved and returned",
    ),
    (
        "kah-cli/src/dispatch.rs",
        "routing::evaluate",
        1,
        "user quotes for one dispatch; an error fails that command visibly and leaves no state",
    ),
    (
        "kah-cli/src/observer_review/claims.rs",
        "routing::evaluate",
        1,
        "fix-14: JEV_CONTEXT_LIMIT goes to the observer, else a gap; the claim never blocks",
    ),
    (
        "kah-cli/src/observer_review/queue/claim_migration/driver.rs",
        "routing::evaluate",
        1,
        "one-time user-authorized migration of claims already screened; re-runnable, out of scope",
    ),
    (
        "kah-cli/src/relay.rs",
        "routing::evaluate",
        1,
        "one user quote and paraphrase; an error makes an unsupported receipt with judgment_error",
    ),
    (
        "kah-cli/src/unattended_observer.rs",
        "routing::evaluate",
        1,
        "#58 open: no size handling; kah run is out of the benchmark, fix after it",
    ),
    (
        "kah-observer/src/judge/routing.rs",
        "jev::Jev {",
        1,
        "the routed call itself",
    ),
    (
        "kah-observer/src/judge/routing.rs",
        "HttpClient::from_home",
        1,
        "the routed call itself",
    ),
    (
        "kah-observer/src/judge/availability/recovery.rs",
        "jev::Jev {",
        1,
        "service probe with a fixed one-line state",
    ),
    (
        "kah-observer/src/judge/availability/recovery.rs",
        "HttpClient::from_home",
        1,
        "service probe with a fixed one-line state",
    ),
    (
        "kah-observer/src/judge/jev/http.rs",
        ".post_raw(",
        1,
        "the client's own request",
    ),
    (
        "kah-cli/src/commit_measure/live.rs",
        "HttpClient::from_home",
        1,
        "measurement; JEV_CONTEXT_LIMIT is recorded as provider_rejected",
    ),
    (
        "kah-cli/src/commit_measure/live.rs",
        ".post_raw(",
        1,
        "measurement; JEV_CONTEXT_LIMIT is recorded as provider_rejected",
    ),
    (
        "kah-cli/src/observer_review/preparation/send.rs",
        "HttpClient::from_home",
        1,
        "preparation::ready::finish sends JEV_CONTEXT_LIMIT back to preparation",
    ),
    (
        "kah-cli/src/observer_review/option1/admission.rs",
        "HttpClient::from_home",
        1,
        "commit_review::transport keeps JEV_CONTEXT_LIMIT as a known_oversized gap",
    ),
    (
        "kah-cli/src/observer_review/option1/selection.rs",
        "HttpClient::from_home",
        1,
        "commit_review::transport keeps JEV_CONTEXT_LIMIT as a known_oversized gap",
    ),
    (
        "kah-cli/src/observer_review/untrusted/worker.rs",
        "HttpClient::from_home",
        1,
        "commit_review gaps; a known_oversized body ends as too_large",
    ),
];

#[test]
fn every_direct_jev_call_is_a_known_site() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut found = BTreeMap::new();
    for entry in fs::read_dir(crates).unwrap() {
        let source = entry.unwrap().path().join("src");
        if source.is_dir() {
            scan(crates, &source, &mut found);
        }
    }
    let known = KNOWN
        .iter()
        .map(|(file, entry, count, note)| {
            assert!(!note.is_empty());
            ((file.to_string(), *entry), *count)
        })
        .collect::<BTreeMap<_, _>>();
    let changed = found
        .keys()
        .chain(known.keys())
        .filter(|site| found.get(*site) != known.get(*site))
        .map(|site| (site, found.get(site), known.get(site)))
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "a direct Jev call changed (site, found, known): {changed:?}. Give it size handling (see claims.rs oversized), then list it in KNOWN with a note"
    );
}

fn scan(crates: &Path, directory: &Path, found: &mut BTreeMap<(String, &'static str), usize>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan(crates, &path, found);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let file = path
            .strip_prefix(crates)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        for entry in ENTRIES {
            let count = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .map(|l| l.matches(entry).count())
                .sum::<usize>();
            if count > 0 {
                found.insert((file.clone(), entry), count);
            }
        }
    }
}
