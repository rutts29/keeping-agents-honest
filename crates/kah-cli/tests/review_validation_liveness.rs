use kah_cli::observer_review::liveness::{Sample, transition};

fn sample(now_ms: u64, cause: &str) -> Sample {
    Sample {
        request: "review-rejection".into(),
        now_ms,
        started_ms: 0,
        native_active: false,
        controller_active: false,
        transcript_version: None,
        report_version: None,
        cause: cause.into(),
    }
}

#[test]
fn verified_rejection_retains_named_validation_block_without_becoming_stalled() {
    let first = transition(None, &sample(0, "idle"));
    let blocked = transition(
        Some(&first),
        &sample(
            30_000,
            "VALIDATION BLOCKED: REVIEW_SOURCE_CHANGED stage=evidence",
        ),
    );
    assert!(!blocked.stalled);
    assert!(blocked.changed);
    let retained = transition(Some(&blocked), &sample(900_000, &blocked.sample.cause));
    assert!(!retained.stalled);
    assert!(!retained.changed);
    let mut native = sample(900_001, &blocked.sample.cause);
    native.transcript_version = Some((123, 900_001));
    assert!(!transition(Some(&retained), &native).stalled);
    let repaired = transition(Some(&retained), &sample(900_001, "idle"));
    assert!(
        repaired.stalled && repaired.changed,
        "cleared validation evidence cannot hide actual idle silence"
    );
}
