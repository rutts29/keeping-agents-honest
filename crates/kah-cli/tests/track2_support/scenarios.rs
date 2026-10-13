#[path = "hook_events.rs"]
mod hook_events;
use super::{fixture::save, journey::Worker};
use kah_cli::observer_review::{
    self as review,
    state::{Request, Store},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{BufRead, BufReader},
};

pub fn verify(worker: &Worker, ids: &[String]) -> Value {
    let store = Store::open_wait(&worker.id).unwrap();
    let requests = ids
        .iter()
        .map(|id| review::workspace::request(&store, id).unwrap())
        .collect::<Vec<_>>();
    let state = store.load().unwrap().unwrap();
    let claims = claims(&requests);
    let anchors = anchors(&requests);
    let cas = cas(&store.directory, &requests);
    let hooks = hooks(worker, &state);
    let idle = requests.iter().any(|r| {
        r.trigger["reconsideration"].is_object() || r.trigger["verification_followup"].is_object()
    });
    let steering = requests
        .iter()
        .any(|r| r.purpose == review::state::Purpose::Steering);
    let bad = bad_events(&store.directory);
    let passed = claims["passed"] == true
        && anchors["passed"] == true
        && cas["passed"] == true
        && hooks["passed"] == true
        && idle
        && steering
        && bad.is_empty()
        && state.deferrals.is_empty();
    let value = json!({"passed":passed,"claims":claims,"historical_anchors":anchors,"cas":cas,
        "native_hooks":hooks,"idle_followup":idle,"steering_completed":steering,"bad_events":bad,"final_deferrals":state.deferrals});
    save(
        &worker
            .root
            .join(format!("{}-scenario-checks.json", worker.provider)),
        &value,
    );
    value
}
fn claims(requests: &[Request]) -> Value {
    let mut identities = BTreeSet::new();
    let mut duplicated = vec![];
    for r in requests {
        if r.trigger["reconsideration"].is_object()
            || r.trigger["verification_followup"].is_object()
        {
            continue;
        }
        for event in r.trigger["events"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["kind"] == "native_phase_claim")
        {
            let source = serde_json::to_string(&event["source"]).unwrap();
            if !identities.insert(source.clone()) {
                duplicated.push(json!({"request":r.id,"source":source}));
            }
        }
    }
    json!({"passed":identities.len()>=3 && duplicated.is_empty(),"unique_native_claims":identities.len(),"duplicated":duplicated})
}
fn anchors(requests: &[Request]) -> Value {
    let mut retained = vec![];
    let mut violations = vec![];
    let mut revisions = BTreeSet::new();
    for r in requests {
        if let Some(revision) = r.intent["revision"].as_u64() {
            revisions.insert(revision);
        }
        for deferral in r.intent["open_deferrals"].as_array().into_iter().flatten() {
            let anchor = &deferral["requirement"];
            let document = r.documents.iter().find(|d| anchor["source"] == d.id);
            let valid = document.is_some_and(|d| {
                anchor["digest"] == d.digest
                    && anchor["uri"] == d.uri
                    && anchor["quote"]
                        .as_str()
                        .is_some_and(|q| !q.is_empty() && d.text.contains(q))
            });
            if !valid {
                violations.push(json!({"request":r.id,"deferral":deferral["id"]}));
            }
            if valid && document.unwrap().kind == "historical_deferral_anchor" {
                retained.push(
                    json!({"request":r.id,"deferral":deferral["id"],"digest":anchor["digest"]}),
                );
            }
        }
    }
    json!({"passed":!retained.is_empty() && revisions.len()>1 && violations.is_empty(),
        "restored":retained,"ledger_revisions":revisions,"violations":violations})
}
fn cas(root: &std::path::Path, requests: &[Request]) -> Value {
    let mut unique = true;
    let mut questions = 0;
    let mut verified = 0;
    for r in requests {
        let mut ids = BTreeSet::new();
        for d in &r.documents {
            unique &= ids.insert(&d.id);
            questions += usize::from(d.id == "pending-questions");
        }
        let archived = root.join("requests").join(format!("{}.json", r.id));
        let expanded: Request = review::source_store::read(&archived).unwrap();
        assert_eq!(
            serde_json::to_value(&expanded).unwrap(),
            serde_json::to_value(r).unwrap(),
            "TRACK2_CAS_SHADOW stage=source"
        );
        verified += 1;
    }
    json!({"passed":unique && questions>0 && verified>0,"unique_source_ids":unique,
        "question_documents":questions,"lossless_requests":verified})
}
fn hooks(worker: &Worker, state: &review::state::State) -> Value {
    let path = state.observer.transcript.as_ref().unwrap();
    let mut native = vec![];
    let mut errors = vec![];
    for line in BufReader::new(fs::File::open(path).unwrap()).lines() {
        let Ok(row) = serde_json::from_str::<Value>(&line.unwrap()) else {
            continue;
        };
        if row["payload"]["type"] == "hook_completed" {
            let run = &row["payload"]["run"];
            if run["source_path"]
                .as_str()
                .is_some_and(|p| p.starts_with(worker.home.to_str().unwrap()))
            {
                let metadata = json!({"id":run["id"],"event":run["event_name"],"status":run["status"],
                    "duration_ms":run["duration_ms"],"source_path":run["source_path"]});
                native.push(metadata.clone());
                if run["status"] != "completed"
                    || run["duration_ms"].as_i64().is_none_or(|n| n >= 2000)
                {
                    errors.push(metadata);
                }
            }
        }
    }
    let observed = fs::read_to_string(
        Store::directory(&worker.id)
            .unwrap()
            .join("hook-observations.jsonl"),
    )
    .unwrap();
    let observed = observed
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    // Claude does not expose the Codex HookRunSummary wire event. Require actual
    // Tracker39 removed per-tool Jev; require the current intake/stop path.
    let events = hook_events::complete(&observed);
    let passed = events && errors.is_empty() && (worker.provider == "claude" || !native.is_empty());
    json!({"passed":passed,"native_hook_runs":native,"errors":errors,"host_event_count":observed.len(),
        "native_duration_source":if worker.provider=="codex" {"HookRunSummary.duration_ms"} else {"not exposed; completed short-timeout native hooks evidenced by host receipts"}})
}
fn bad_events(root: &std::path::Path) -> Vec<Value> {
    let mut bad = vec![];
    for name in [
        "gap.json",
        "liveness/events.jsonl",
        "liveness/progress.json",
    ] {
        let text = fs::read_to_string(root.join(name)).unwrap_or_default();
        for code in [
            "STORE_BUSY",
            "REVIEW_OBSERVER_STALLED",
            "REVIEW_HOOK_TIMEOUT",
            "REVIEW_SOURCE_ID",
            "REVIEW_SOURCE_DIGEST",
        ] {
            if text.contains(code) {
                bad.push(json!({"source":name,"code":code}));
            }
        }
    }
    bad
}
