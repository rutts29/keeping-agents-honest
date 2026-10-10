//! Offline integration over the retained native commit-scope corpus.
use kah_cli::observer_review::{
    preparation::Job,
    queue::{self, Wake},
    source_store,
    state::{Purpose, Request, Store},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[test]
#[ignore = "retained native corpus; offline, no Jev or native inference"]
fn retained_context_jobs_stay_individual_and_captured_reviews_stay_whole() {
    let corpus = std::path::PathBuf::from(std::env::var_os("KAH_PACKET_REMOVAL_CORPUS").unwrap());
    let evidence = std::path::PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    let private = std::path::PathBuf::from(std::env::var_os("XDG_STATE_HOME").unwrap());
    assert!(private.starts_with("/mnt/kah-fixtures/cargo/test-fixtures"));
    let worker = format!("packet-removal-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    assert!(store.directory.starts_with(&private));
    let contexts = contexts(&corpus);
    for job in &contexts {
        queue::enqueue(&worker, Purpose::Preparation, &job.decision,
            json!({"preparation":{"purpose":"checkpoint","state":job.state,"questions":job.questions,"reason":job.reason}})).unwrap();
    }
    for job in &contexts {
        let (wake, paths) = claim(&worker, &store);
        assert_eq!(wake.trigger["preparation"]["state"], job.state);
        assert_eq!(
            wake.trigger["preparation"]["questions"],
            serde_json::to_value(&job.questions).unwrap()
        );
        assert!(wake.trigger.get("preparation_packets").is_none());
        retain(&store, &paths);
    }
    let snapshots = snapshots(&worker, &store, &corpus);
    assert!(queue::header_entries(&worker).unwrap().is_empty());
    let hashes = snapshots
        .iter()
        .map(|v| format!("{:x}", Sha256::digest(serde_json::to_vec(v).unwrap())))
        .collect::<Vec<_>>();
    fs::create_dir_all(&evidence).unwrap();
    kah_cli::unattended_report::save(&evidence, &format!("{worker}.json"),
        &json!({"passed":true,"corpus":corpus,"contexts":contexts.iter().map(|j|&j.decision).collect::<Vec<_>>(),
            "captured_request_digests":hashes,"private_store":store.directory,"individual_claims":4,
            "originals_retained":true,"jev_calls":0,"native_calls":0,"final_acceptance":false})).unwrap();
}

fn contexts(corpus: &Path) -> Vec<Job> {
    let mut jobs = fs::read_dir(corpus.join("review"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("preparation-review-")
        })
        .collect::<Vec<_>>();
    jobs.sort();
    assert!(jobs.len() >= 2);
    jobs[..2]
        .iter()
        .map(|p| source_store::read::<Job>(p).unwrap())
        .collect()
}

fn snapshots(worker: &str, store: &Store, corpus: &Path) -> Vec<Value> {
    let mut snapshots = vec![];
    let paths: [String; 2] =
        serde_json::from_slice(&fs::read(corpus.join("fixture-requests.json")).unwrap()).unwrap();
    for (name, purpose) in paths.iter().zip([Purpose::Drift, Purpose::Checkpoint]) {
        let relative = Path::new(name);
        assert!(!relative.as_os_str().is_empty());
        assert!(
            relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        );
        let mut request: Request = source_store::read(&corpus.join(relative)).unwrap();
        assert_eq!(request.purpose, purpose);
        request.worker = worker.into();
        let value = serde_json::to_value(&request).unwrap();
        queue::enqueue(
            worker,
            request.purpose,
            &request.turn,
            json!({"captured_snapshot":value}),
        )
        .unwrap();
        snapshots.push(value);
    }
    // Gate priority admits the checkpoint before the drift request,
    // while retaining both complete snapshots.
    for snapshot in snapshots.iter().rev() {
        let (wake, paths) = claim(worker, store);
        assert_eq!(&wake.trigger["captured_snapshot"], snapshot);
        retain(store, &paths);
    }
    snapshots
}

fn claim(worker: &str, store: &Store) -> (Wake, Vec<std::path::PathBuf>) {
    let (wake, paths) = queue::claim_batch(worker).unwrap().unwrap();
    assert_eq!(paths.len(), 1);
    let original: Wake = source_store::read(&paths[0]).unwrap();
    // Queue timestamps are controller metadata; the complete original payload stays intact.
    let mut trigger = wake.trigger.clone();
    trigger.as_object_mut().unwrap().remove("queued_at_ms");
    assert_eq!(trigger, original.trigger);
    assert_eq!(wake.turn, original.turn);
    assert_eq!(wake.purpose, original.purpose);
    assert!(paths[0].starts_with(&store.directory));
    (wake, paths)
}

fn retain(store: &Store, paths: &[std::path::PathBuf]) {
    let archive = store.directory.join("covered-triggers");
    fs::create_dir_all(&archive).unwrap();
    for path in paths {
        let target = archive.join(Path::new(path).file_name().unwrap());
        let before = fs::read(path).unwrap();
        fs::rename(path, &target).unwrap();
        assert_eq!(fs::read(target).unwrap(), before);
    }
}
