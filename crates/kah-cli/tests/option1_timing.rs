//! Native confirmation/consume boundary only: no native agent or Jev call.
use kah_cli::observer_review::{
    option1, source_store,
    state::{Request, State, Store},
};
use kah_observer::commit_review::{DoubtLine, classify};
use serde_json::{Value, json};
use std::fs;

#[test]
fn native_confirmation_has_a_judged_receipt_after_consumption() {
    std::env::var_os("XDG_STATE_HOME").expect("private check-tests state required");
    let worker = format!("option1-timing-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    let (state, request) = fixture(&root, &worker);
    store.save(&state).unwrap();
    let policy: DoubtLine =
        serde_json::from_str(include_str!("../../../assets/observer/commit-policy.json")).unwrap();
    let record = classify(
        &worker,
        request.head.as_deref().unwrap(),
        &[json!({"request":"literal-receipt","question":"r","noul":0.95})],
        &policy,
    )
    .unwrap();
    option1::journal::save(&record).unwrap();
    let report = root.join("workspace/outbox/submitted-review-1-1.json");
    fs::write(&report,json!({"request":request.id,"result":{"stage":"confirmation","rows":[{
        "requirement":"r","finding":"No content was cut.","status":"satisfied","kind":"not_violated","quotes":[]
    }]}}).to_string()).unwrap();
    kah_cli::unattended_report::save(
        &store.directory,
        "report-ready.json",
        &json!({"request":request.id}),
    )
    .unwrap();
    option1::consume::run(store, state, request).unwrap();
    let timing: Value = source_store::read(&root.join("stage-timings/review-1-1.json")).unwrap();
    let judged = timing["at_ms"]["judged"]
        .as_u64()
        .expect("persisted verdict receipt");
    let consumed = timing["at_ms"]["consumed"]
        .as_u64()
        .expect("finished consume receipt");
    assert!(judged <= consumed);
    let final_state = Store::read(&worker).unwrap().unwrap();
    assert!(final_state.pending.is_none());
    assert!(!report.exists());
    assert!(
        option1::journal::load(&worker, &"a".repeat(40))
            .unwrap()
            .unwrap()
            .native
            .is_some()
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
fn fixture(root: &std::path::Path, worker: &str) -> (State, Request) {
    let workspace = root.join("workspace");
    kah_core::run_store::ensure_private_directory(&workspace.join("outbox")).unwrap();
    let state: State = serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":"observer-unit","pane":null,"name":"fixture","workspace":workspace,
            "model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":{"id":"review-1-1","purpose":"checkpoint","turn":"t","head":"a".repeat(40),"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let request: Request = serde_json::from_value(json!({"id":"review-1-1","worker":worker,"purpose":"checkpoint",
        "turn":"t","head":"a".repeat(40),"intent":{},"documents":[],"created_ms":1,
        "trigger":{"option1":{"stage":"confirmation","input":{
            "revision":"a".repeat(40),"baseline":null,"checkpoint":false,"code_changed":true,
            "intent":{"goal":"Retain source.","items":{},"retired":[],"permissions":{},"conflicts":[]},
            "impacted":[],"rules":{"r":{"authority":"r","unconditional_prohibition":true,"prohibition":true,"literal_check":true,"hunks":[]}},
            "hunks":{},"files":{},"matrix":[],"carried":{"authority_digest":"authority-unit"}}
    }}})).unwrap();
    (state, request)
}
