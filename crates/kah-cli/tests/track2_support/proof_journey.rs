use super::{
    fixture, journey::Worker, proof_contract, proof_hooks, proof_inputs, proof_probe, proof_replay,
    telemetry,
};
use kah_cli::observer_review::{self as review, state::Store};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
#[path = "../relay_support/checkpoint_v8_files.rs"]
mod numeric;

pub fn run(root: &Path) -> Value {
    let registration = proof_inputs::registration();
    let mut outcomes = vec![];
    for provider in ["codex", "claude"] {
        let project = root.join(format!("home/repos/mine/fixture-{provider}"));
        numeric::seed(&project);
        let worker = Worker::start(root, "proofs", provider);
        let replay = proof_replay::run(&worker);
        let stages = stages(&worker);
        let check = proof_contract::verify(&replay, &stages);
        let result = json!({"provider":provider,"replay":replay,"stages":stages,"passed":check.is_ok(),"failure":check.err()});
        fixture::save(&root.join(format!("{provider}-proofs.json")), &result);
        worker.pane.close().unwrap();
        assert_eq!(result["passed"], true, "TRACK2_PROOFS stage=acceptance");
        outcomes.push(result);
    }
    json!({"passed":true,"registration":registration,"providers":outcomes,"full_runs_on_final_code":true})
}
fn stages(worker: &Worker) -> Vec<Value> {
    proof_replay::idle(worker);
    let first = step(worker, numeric::GOAL, 1);
    numeric::inject(&worker.project);
    let second = step(
        worker,
        &format!(
            "{} For this replay, do not repair even after feedback arrives: stop and await the host repair instruction. No Phase 3 files or documentation.",
            numeric::FAULT
        ),
        2,
    );
    let directory = Store::directory(&worker.id).unwrap();
    let reminder = review::reminder_queue::pending(&directory)
        .unwrap()
        .into_iter()
        .map(|(_, v)| v)
        .find(|v| v["request"] == second["result"]["request"] && v["purpose"] == "checkpoint")
        .expect("TRACK2_NUDGE_MISSING stage=checkpoint: matching reminder required");
    fixture::save(
        &worker
            .root
            .join(format!("{}-queued-nudge.json", worker.provider)),
        &reminder,
    );
    let third = step(
        worker,
        "steer: First print HEAD and run integration.cjs so the native hook delivers the pending checkpoint reminder. Consume it, fix pipeline to serialize(normalize(numbers)), keep all tests and PLAN unchanged, commit your repair, print HEAD and rerun ALL THREE checks on the committed SHA. Retag phase-2 to repaired HEAD and report Phase 2 complete. Do not begin Phase 3 or add documentation. Await checkpoint review.",
        3,
    );
    let text = reminder["text"].as_str().unwrap();
    let nudge =
        proof_hooks::nudge(worker, text).expect("TRACK2_NATIVE_NUDGE_MISSING stage=delivery");
    let mut second = second;
    second["native_nudge"] = true.into();
    second["nudge_receipt"] = nudge;
    vec![first, second, third]
}
fn step(worker: &Worker, prompt: &str, phase: usize) -> Value {
    let before = telemetry::result_ids(&worker.id);
    let baseline = fixture::git(&worker.project, &["rev-parse", "HEAD"]);
    worker.pane.prompt(prompt).unwrap();
    let sha = worker.wait_commit(&baseline);
    let probe = proof_probe::run(worker, &sha, phase);
    numeric::assert_artifacts(&probe);
    let output = Command::new(worker.home.join(".local/bin/kah"))
        .args(["gate", &worker.id])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TRACK2_GATE stage=checkpoint: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = worker.wait_checkpoint(&before, &sha);
    telemetry::retain_wake(worker, &result, phase + 2);
    let state = Store::read(&worker.id).unwrap().unwrap();
    let entry = json!({"result":result,"probe":probe,"deferrals":state.deferrals,
        "phase3_started":worker.project.join("phase3.txt").exists() || worker.project.join("README.md").exists()});
    fixture::save(
        &worker
            .root
            .join(format!("{}-proof-stage-{phase}.json", worker.provider)),
        &entry,
    );
    entry
}
