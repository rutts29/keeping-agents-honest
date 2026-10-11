//! One complete private Herdr lifecycle; production batch, fake transport, no agents.
use super::{bundle_cost, fixture::save};
use kah_observer::judge::batch::{Batch, Mode, RawResponse, Tokens, Transport, WireRequest};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

struct Fake(AtomicUsize);
impl Transport for Fake {
    fn exchange(&self, _: &WireRequest) -> kah_observer::judge::batch::Result<RawResponse> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(RawResponse {
            status: 200,
            headers: Default::default(),
            body: json!({"usage":{"input_tokens":100},"fake_transport":true}).to_string(),
        })
    }
}
pub fn run(root: &Path) -> Value {
    assert_eq!(std::env::var("KAH_TRACK2_LIFECYCLE").as_deref(), Ok("1"));
    let home = root.join("home");
    let batch_root = home.join(".local/state/kah/fake-batch");
    let rate_root = home.join(".local/state/kah/fake-rate");
    let transport = Fake(AtomicUsize::new(0));
    let request = WireRequest {
        id: "dry-fake".into(),
        body: "{\"state\":{},\"questions\":{}}".into(),
        tokens: Tokens {
            estimated: 100.0,
            source: "fake-only".into(),
            recorded_exact: Some(100),
            input_utf8_bytes: 0,
        },
    };
    let batch = Batch::new(&batch_root, &rate_root).unwrap();
    batch
        .run(std::slice::from_ref(&request), &transport, Mode::Sequential)
        .unwrap();
    let baseline = bundle_cost::audited_sample(root, &home);
    let skipped = churn(root, &home, &batch_root);
    let stable = bundle_cost::audited_sample(root, &home);
    assert_eq!(baseline["exact_input_tokens"], 100);
    assert_eq!(stable["exact_input_tokens"], baseline["exact_input_tokens"]);
    assert_eq!(stable["usage_record_lost"], false);
    let loss = prove_loss(root, &home, &batch_root);
    let cached = batch.run(&[request], &transport, Mode::Sequential).unwrap();
    assert_eq!(transport.0.load(Ordering::Relaxed), 1);
    assert!(cached.responses[0].reused);
    let result = json!({"passed":true,"dry_run":true,"native_agents":0,"real_jev_calls":0,
        "real_jev_usd":0,"fake_transport_calls":1,"fake_exact_input_tokens":100,
        "vanished_path_events":skipped,"monitors_stopped":true,"usage_loss_probe":loss,
        "cached_response_reused":true,"gate_passed":false});
    save(&root.join("dry-lifecycle.json"), &result);
    result
}
fn churn(root: &Path, home: &Path, batch: &Path) -> usize {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    let scratch = batch.join("scratch");
    let writer = std::thread::spawn(move || {
        while flag.load(Ordering::Relaxed) {
            fs::create_dir_all(scratch.join("nested")).unwrap();
            fs::write(scratch.join("nested/transient"), b"scratch").unwrap();
            fs::remove_dir_all(&scratch).unwrap();
        }
    });
    let samples = (0..200)
        .map(|_| {
            let result = bundle_cost::audited_sample(root, home);
            assert_eq!(result["exact_input_tokens"], 100);
            assert_eq!(result["usage_record_lost"], false);
            result
        })
        .collect::<Vec<_>>();
    running.store(false, Ordering::Relaxed);
    writer.join().unwrap();
    let skipped = samples
        .iter()
        .map(|v| v["vanished_paths"].as_array().unwrap().len())
        .sum();
    save(&root.join("dry-churn-samples.json"), &json!(samples));
    skipped
}
fn prove_loss(root: &Path, home: &Path, batch: &Path) -> Value {
    let ledger = batch.join("events.jsonl");
    let bytes = fs::read(&ledger).unwrap();
    fs::remove_file(&ledger).unwrap();
    let lost = bundle_cost::audited_sample(root, home);
    assert_eq!(lost["usage_record_lost"], true);
    assert_eq!(
        lost["exact_input_tokens"], 100,
        "known charge must survive lost telemetry"
    );
    fs::write(&ledger, bytes).unwrap();
    assert_eq!(
        bundle_cost::audited_sample(root, home)["usage_record_lost"],
        false
    );
    json!({"lost_record_detected":true,"charge_retained":true,"restored":true})
}
