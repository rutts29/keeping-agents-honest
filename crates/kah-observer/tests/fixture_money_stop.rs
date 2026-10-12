//! $0 transport-only regression: opt-in money bounds, unchanged body/retry policy.
use kah_observer::judge::{
    availability,
    batch::{Batch, Mode, RawResponse, Tokens, Transport, WireRequest},
};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};
struct Fake {
    calls: AtomicUsize,
    bodies: Mutex<Vec<String>>,
    retry: bool,
}
impl Transport for Fake {
    fn exchange(&self, request: &WireRequest) -> kah_observer::judge::batch::Result<RawResponse> {
        self.bodies.lock().unwrap().push(request.body.clone());
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let status = if self.retry {
            [429, 529, 503].get(n).copied().unwrap_or(200)
        } else {
            200
        };
        Ok(response(status))
    }
}
fn response(status: u16) -> RawResponse {
    RawResponse {
        status,
        headers: [("retry-after-ms".into(), "0".into())].into(),
        body: if status == 200 {
            json!({"usage":{"input_tokens":1000},"fake_transport":true}).to_string()
        } else {
            json!({"error":"recorded outage/throttle fixture"}).to_string()
        },
    }
}
fn request(id: &str, estimate: f64) -> WireRequest {
    WireRequest {id:id.into(),body:json!({"model":"jev-1.13.0","state":{"whole":"Keep all bytes"},"questions":{"q":{"type":"noul","instructions":"Is whole preserved?"}}}).to_string(),
        tokens:Tokens {estimated:estimate,source:"synthetic $0 fixture reservation".into(),recorded_exact:None,input_utf8_bytes:0} }
}
fn fake(retry: bool) -> Fake {
    Fake {
        calls: AtomicUsize::new(0),
        bodies: Mutex::new(vec![]),
        retry,
    }
}
fn seed(root: &Path, tokens: u64) {
    fs::create_dir_all(root).unwrap();
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        root.join("total.json"),
        json!({"input_tokens":tokens,"synthetic_test_ledger":true}).to_string(),
    )
    .unwrap();
    fs::set_permissions(root.join("total.json"), fs::Permissions::from_mode(0o600)).unwrap();
}
fn total(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("total.json")).unwrap()).unwrap()
}
fn batch(root: &Path, name: &str) -> Batch {
    Batch::new(&root.join(name), &root.join("rate")).unwrap()
}
#[test]
fn fixture_money_stop_respects_actual_and_reserved_spend_without_changing_transport_paths() {
    if let Ok(mode) = std::env::var("KAH_MONEY_CHILD_MODE") {
        let root = PathBuf::from(std::env::var_os("KAH_FIXTURE_BUDGET_ROOT").unwrap());
        match mode.as_str() {
            "closed" => closed(&root),
            "retry" => retry(&root),
            "concurrent" => concurrent(&root),
            "timeout" => timeout(&root),
            "usage-gap" => usage_gap(&root),
            _ => panic!("unknown fixture mode"),
        };
        return;
    }
    let root = std::env::temp_dir().join(format!("kah-money-test-{}", std::process::id()));
    for mode in ["closed", "retry", "concurrent", "timeout", "usage-gap"] {
        let ledger = root.join(mode);
        seed(&ledger, if mode == "closed" { 2000 } else { 0 });
        assert!(Command::new(std::env::current_exe().unwrap())
            .args(["--exact","fixture_money_stop_respects_actual_and_reserved_spend_without_changing_transport_paths","--nocapture"])
            .env("KAH_MONEY_CHILD_MODE",mode).env("KAH_FIXTURE_BUDGET_ROOT",&ledger)
            .env("JEV_FIXTURE_MONEY_STOP","0.000126").status().unwrap().success());
    }
    fs::remove_dir_all(root).unwrap();
}
fn closed(root: &Path) {
    let fake = fake(false);
    let error = batch(root, "closed")
        .run(&[request("closed", 2000.)], &fake, Mode::Sequential)
        .err()
        .expect("reserve would exceed approved stop");
    assert_eq!(error.code, "JEV_FIXTURE_MONEY_STOP");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert_eq!(total(root)["input_tokens"], 2000);
}
fn retry(root: &Path) {
    let fake = fake(true);
    let batch = batch(root, "retry");
    let request = request("unchanged", 1000.);
    let error = batch
        .run(std::slice::from_ref(&request), &fake, Mode::Sequential)
        .err()
        .unwrap();
    assert_eq!(error.status, Some(503));
    assert!(availability::outage(&error.to_string()));
    let recovered = batch
        .run(std::slice::from_ref(&request), &fake, Mode::Sequential)
        .unwrap();
    assert!(!recovered.responses[0].reused);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 4);
    assert!(
        fake.bodies
            .lock()
            .unwrap()
            .iter()
            .all(|b| b == &request.body)
    );
    assert_eq!(total(root)["input_tokens"], 1000);
    assert!(total(root)["reserved"].as_object().unwrap().is_empty());
    batch
        .run(&[self::request("next", 2000.)], &fake, Mode::Sequential)
        .unwrap();
    assert_eq!(total(root)["input_tokens"], 2000);
    let error = batch
        .run(&[self::request("refused", 2000.)], &fake, Mode::Sequential)
        .err()
        .unwrap();
    assert_eq!(error.code, "JEV_FIXTURE_MONEY_STOP");
    let cached = batch.run(&[request], &fake, Mode::Sequential).unwrap();
    assert!(cached.responses[0].reused);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 5);
}
struct Held {
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl Transport for Held {
    fn exchange(&self, _: &WireRequest) -> kah_observer::judge::batch::Result<RawResponse> {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Ok(response(200))
    }
}
fn concurrent(root: &Path) {
    let (tx, rx) = mpsc::sync_channel(0);
    let (release, wait) = mpsc::sync_channel(0);
    let held = Held {
        entered: tx,
        release: Mutex::new(wait),
    };
    let first = batch(root, "first");
    std::thread::scope(|scope| {
        let running =
            scope.spawn(|| first.run(&[request("in-flight", 2000.)], &held, Mode::Sequential));
        rx.recv().unwrap();
        let fake = fake(false);
        let refusal = batch(root, "second")
            .run(&[request("parallel", 2000.)], &fake, Mode::Sequential)
            .err();
        release.send(()).unwrap();
        running.join().unwrap().unwrap();
        assert_eq!(refusal.unwrap().code, "JEV_FIXTURE_MONEY_STOP");
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    });
    assert_eq!(total(root)["input_tokens"], 1000);
    assert!(total(root)["reserved"].as_object().unwrap().is_empty());
}
struct Broken(bool);
impl Transport for Broken {
    fn exchange(&self, _: &WireRequest) -> kah_observer::judge::batch::Result<RawResponse> {
        if self.0 {
            Err(kah_observer::judge::Error::new("JEV_HTTP_TIMEOUT"))
        } else {
            Ok(RawResponse {
                status: 200,
                headers: Default::default(),
                body: "{\"fake_transport\":true}".into(),
            })
        }
    }
}
fn timeout(root: &Path) {
    let batch = batch(root, "timeout");
    let request = request("timeout", 1000.);
    let error = batch
        .run(
            std::slice::from_ref(&request),
            &Broken(true),
            Mode::Sequential,
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "JEV_HTTP_TIMEOUT");
    assert!(availability::outage(&error.to_string()));
    assert!(total(root)["reserved"].as_object().unwrap().is_empty());
    let fake = fake(false);
    batch.run(&[request], &fake, Mode::Sequential).unwrap();
    assert_eq!(total(root)["input_tokens"], 1000);
}
fn usage_gap(root: &Path) {
    let batch = batch(root, "usage-gap");
    batch
        .run(
            &[request("missing", 1000.)],
            &Broken(false),
            Mode::Sequential,
        )
        .unwrap();
    assert_eq!(total(root)["usage_gap"], true);
    assert_eq!(total(root)["reserved"].as_object().unwrap().len(), 1);
    let fake = fake(false);
    let error = batch
        .run(&[request("next", 1000.)], &fake, Mode::Sequential)
        .err()
        .unwrap();
    assert_eq!(error.code, "JEV_FIXTURE_MONEY_STOP");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}
