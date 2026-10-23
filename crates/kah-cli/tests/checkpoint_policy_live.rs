//! Opt-in live Jev check of the checkpoint policy on four authored cases:
//! repaired work, a closed deferral and a genuine later dependency pass;
//! broken current work does not.
use kah_observer::judge::{
    Questions,
    routing::{self, Backend},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

const GOAL: &str =
    "Phase 1: build a CSV parser with tests. Phase 2: add a renderer that uses the parser.";
const PLAN: &str = "Phase 2 adds the renderer; it needs the finished parser.";
const HEAD: &str = "6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f";

#[test]
#[ignore = "live Jev policy check on authored checkpoint cases; opt-in, needs a Jev key"]
fn partial_phase_policy_replay() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let cases = [
        ("repaired-failure", repaired(), true),
        ("closed-deferral", closed(), true),
        ("genuine-future", future(), true),
        ("broken-current", broken(), false),
    ];
    let mut results = vec![];
    for (name, (actions, deferrals), expected) in cases {
        results.push(case(&home, name, actions, deferrals, expected));
    }
    let passed = results.iter().all(|r| r["matchesExpected"] == true);
    if let Some(evidence) = std::env::var_os("KAH_GATE_EVIDENCE") {
        fs::write(PathBuf::from(evidence).join(format!("checkpoint-policy-v{}-replay.json",kah_observer::checkpoint::VERSION)),serde_json::to_vec_pretty(&json!({"cases":results,"checkpointQuestionSet":kah_observer::checkpoint::VERSION,"passed":passed})).unwrap()).unwrap();
    }
    assert!(passed, "policy cases unproven: {results:?}");
}

fn action(topic: &str, claim: &str, quote: &str) -> Value {
    json!({"id":format!("verified-{topic}"),"kind":"verifiedEvidence","evidence":{"topic":topic,
        "model_claim_not_a_decision":claim,"citations":[{"source":"native:check","quote":quote}]}})
}
fn renderer_deferral(claim: &str) -> Value {
    json!({"id":"renderer","phase":"Phase 2","introduced_at":"review-1700000100000-1",
        "requirement":{"source":"plan-0","uri":"PLAN.md","kind":"plan","quote":PLAN},
        "dependency":{"source":"plan-0","uri":"PLAN.md","kind":"plan","quote":PLAN},
        "evidence":{"topic":"deferral","model_claim_not_a_decision":claim,
            "citations":[{"source":"plan-0","quote":PLAN}]}})
}
fn repaired() -> (Value, Vec<Value>) {
    let actions = json!([
        action(
            "integration",
            "An earlier parser test failed; the fix on this head makes every parser test pass.",
            "node parser.test.cjs\nparser: 6 passed, 0 failed"
        ),
        action(
            "intent",
            "The parser keeps quoted fields exactly, as the user asked.",
            "assert.equal(parse('\"a,b\",c')[0], 'a,b')"
        ),
    ]);
    (actions, vec![])
}
fn closed() -> (Value, Vec<Value>) {
    let actions = json!([
        action(
            "integration",
            "The renderer was deferred, then delivered: it uses the parser and its tests pass.",
            "node render.test.cjs\nrender: 4 passed, 0 failed"
        ),
        action(
            "intent",
            "Parser tests also pass on this head.",
            "parser: 6 passed, 0 failed"
        ),
    ]);
    let deferral =
        renderer_deferral("The renderer was deferred to phase 2 and is now delivered and tested.");
    (actions, vec![deferral])
}
fn future() -> (Value, Vec<Value>) {
    let actions = json!([action(
        "integration",
        "The phase 1 parser is complete and its tests pass.",
        "parser: 6 passed, 0 failed"
    )]);
    let deferral = renderer_deferral(
        "The renderer is absent now; the plan puts it in phase 2 and it needs the finished parser.",
    );
    (actions, vec![deferral])
}
fn broken() -> (Value, Vec<Value>) {
    let actions = json!([action(
        "integration",
        "The parser is the phase 1 deliverable, and two of its tests fail on this head.",
        "node parser.test.cjs\nparser: 4 passed, 2 failed"
    )]);
    (actions, vec![])
}

fn case(
    home: &std::path::Path,
    name: &str,
    actions: Value,
    deferrals: Vec<Value>,
    expected: bool,
) -> Value {
    let intent = json!({"revision":2,"goal":GOAL,
        "items":[{"id":"user-1","text":"Keep quoted fields exactly as written."}]});
    let trigger = json!({"events":[{"kind":"user_gate","source":"host_cli"}]});
    let state = kah_cli::observer_review::checkpoint::decision_context(
        &intent,
        &trigger,
        Some(HEAD),
        &actions,
        json!(deferrals),
    );
    let mut questions: Questions = kah_observer::checkpoint::questions();
    for d in &deferrals {
        questions.extend(kah_observer::checkpoint::deferral_questions(
            d["id"].as_str().unwrap(),
        ));
    }
    let judgment = routing::evaluate(home, Backend::Jev, "jev-latest", &state, &questions).unwrap();
    let outcome = kah_cli::observer_review::checkpoint::assess(&judgment, deferrals);
    json!({"case":name,"state":state,"judgment":judgment,"outcome":outcome,"expectedPass":expected,
        "matchesExpected":(outcome["passed"]==true)==expected})
}
