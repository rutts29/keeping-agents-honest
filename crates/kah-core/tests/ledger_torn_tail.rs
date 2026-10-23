use kah_core::Ledger;
use std::fs::{self, OpenOptions};
use std::io::Write;

#[test]
fn an_unfinished_last_line_is_ignored_and_repaired_by_the_next_record() {
    for (case, tail) in [
        ("prefix", &b"{\"schema_version\":1,\"sequ"[..]),
        ("multibyte", &b"{\"text\":\"caf\xc3"[..]),
    ] {
        let path =
            std::env::temp_dir().join(format!("kah-ledger-torn-{case}-{}", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut ledger = Ledger::create(&path, "goal", [("R1", "first")]).unwrap();
        ledger.satisfy("R1", "done").unwrap();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(tail)
            .unwrap();

        let mut reopened = Ledger::open(&path).unwrap();
        assert!(
            reopened
                .snapshot()
                .unwrap()
                .unresolved_requirement_ids()
                .is_empty()
        );
        let id = reopened.steer("second").unwrap();
        let snapshot = Ledger::open(&path).unwrap().snapshot().unwrap();
        assert_eq!(snapshot.unresolved_requirement_ids(), [id], "{case}");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n') && text.lines().count() == 3, "{case}");
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn a_malformed_terminated_line_still_fails_closed() {
    let path = std::env::temp_dir().join(format!("kah-ledger-interior-{}", std::process::id()));
    let _ = fs::remove_file(&path);
    Ledger::create(&path, "goal", [("R1", "first")]).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"garbage\n")
        .unwrap();
    let error = Ledger::open(&path).unwrap_err().to_string();
    fs::remove_file(path).unwrap();
    assert!(error.contains("LEDGER_JSON"), "{error}");
}

#[test]
fn concurrent_writers_do_not_lose_records_when_repairing_an_unfinished_line() {
    use std::sync::{Arc, Barrier};
    const WRITERS: usize = 6;
    const STEPS: usize = 8;
    let path = std::env::temp_dir().join(format!("kah-ledger-race-{}", std::process::id()));
    let _ = fs::remove_file(&path);
    Ledger::create(&path, "goal", [("R1", "first")]).unwrap();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"schema_version\":1,\"sequ")
        .unwrap();

    let start = Arc::new(Barrier::new(WRITERS));
    let workers: Vec<_> = (0..WRITERS)
        .map(|_| {
            let (path, start) = (path.clone(), Arc::clone(&start));
            std::thread::spawn(move || {
                let mut ledger = Ledger::open(&path).unwrap();
                start.wait();
                (0..STEPS)
                    .map(|step| ledger.steer(format!("requirement {step}")).unwrap())
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut ids: Vec<String> = workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        WRITERS * STEPS,
        "every acknowledged id is unique"
    );

    let snapshot = Ledger::open(&path).unwrap().snapshot().unwrap();
    assert_eq!(snapshot.requirements().len(), 1 + WRITERS * STEPS);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 1 + WRITERS * STEPS);
    fs::remove_file(path).unwrap();
}
