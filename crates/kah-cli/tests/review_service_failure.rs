use kah_cli::observer_review::{
    failure::{self, Origin},
    redelivery,
};
#[test]
fn payment_liveness_is_a_service_outage_not_a_stall_or_report_correction() {
    use kah_cli::observer_review::{
        liveness::{Sample, transition},
        service,
    };
    let mut sample = Sample {
        request: "retained".into(),
        now_ms: 1,
        started_ms: 1,
        native_active: false,
        controller_active: false,
        host_owned: false,
        transcript_version: None,
        report_version: None,
        cause: service::cause("JEV_SERVICE_UNAVAILABLE stage=review_service http_status=402"),
    };
    assert!(
        sample
            .cause
            .starts_with("SERVICE OUTAGE: Jev unavailable: payment required")
    );
    assert!(!sample.cause.contains("VALIDATION BLOCKED"));
    let prior = transition(None, &sample);
    sample.now_ms = 1000000;
    let next = transition(Some(&prior), &sample);
    assert!(!next.stalled);
    assert!(!next.changed);
    for error in [
        "JEV_HTTP_TIMEOUT stage=judge",
        "JEV_HTTP_STATUS stage=judge http_status=503",
        "JEV_HTTP_STATUS stage=judge http_status=529",
    ] {
        sample.cause = service::cause(error);
        assert!(sample.cause.starts_with("SERVICE OUTAGE:"));
        let prior = transition(None, &sample);
        sample.now_ms += 1000000;
        assert!(!transition(Some(&prior), &sample).stalled);
        assert!(!redelivery::retryable(Origin::ReportValidation, error));
    }
    assert!(
        service::cause("REVIEW_REQUIREMENT_EMPTY stage=report").starts_with("VALIDATION BLOCKED")
    );
}

#[test]
fn whole_file_expansion_is_a_host_handoff_never_an_idle_observer_stall() {
    use kah_cli::observer_review::liveness::{Sample, description, transition};
    let mut sample = Sample {
        request: "same-native-request".into(),
        now_ms: 1,
        started_ms: 1,
        native_active: false,
        controller_active: true,
        host_owned: false,
        transcript_version: None,
        report_version: None,
        cause: "VALIDATION BLOCKED: REVIEW_SOURCE_OUTSIDE_PACKET stage=report".into(),
    };
    let prior = transition(None, &sample);
    sample.now_ms = 1000000;
    assert!(!transition(Some(&prior), &sample).stalled);
    assert_eq!(description(&sample).0, "host-expansion");
    assert!(description(&sample).1.contains("awaiting host publication"));
    assert!(!description(&sample).1.contains("retained for correction"));
}
#[test]
fn jev_payment_required_is_never_a_thinker_correction_even_in_a_report_boundary() {
    let error = failure::validation(Origin::ReportValidation, || -> kah_cli::Result<()> {
        Err(kah_observer::judge::Error {
            code: "JEV_HTTP_STATUS".into(),
            stage: "judge_http",
            status: Some(402),
            numeric_detail: None,
        }
        .into())
    })
    .unwrap_err();
    assert_eq!(failure::origin(error.as_ref()), Origin::Host);
    assert!(!redelivery::retryable(
        failure::origin(error.as_ref()),
        &error.to_string()
    ));
    // Existing on-disk gaps may have the incorrect old origin; never re-prompt.
    assert!(!redelivery::retryable(
        Origin::ReportValidation,
        "JEV_HTTP_STATUS stage=judge_http http_status=402"
    ));
}

#[test]
fn payment_outage_keeps_queue_and_request_without_correction_and_recovers_on_success() {
    for error in [
        "JEV_HTTP_STATUS stage=judge_http http_status=402",
        "JEV_HTTP_STATUS stage=judge_http http_status=503",
        "JEV_HTTP_STATUS stage=judge_http http_status=529",
        "JEV_HTTP_TIMEOUT stage=judge",
    ] {
        service_case(error);
    }
}
fn service_case(error: &str) {
    let home = std::env::temp_dir().join(format!("kah-review-service-{}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "service_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("HOME", &home)
        .env_remove("XDG_STATE_HOME")
        .env("KAH_SERVICE_CHILD", "1")
        .env("KAH_SERVICE_ERROR", error)
        .output()
        .unwrap();
    std::fs::remove_dir_all(home).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn service_child() {
    if std::env::var_os("KAH_SERVICE_CHILD").is_none() {
        return;
    }
    use kah_cli::observer_review::{
        queue, reminder_queue, service,
        state::{Purpose, State, Store},
    };
    use kah_observer::judge::availability;
    use serde_json::json;
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let worker = "service-fixture";
    let store = Store::open(worker).unwrap();
    let state:State = serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":home,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":home,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":{"id":"request-402","purpose":"drift","turn":"t","head":null,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let root = store.directory.clone();
    kah_cli::unattended_report::save(&root,"config.json",&json!({"enabled":true,"provider":"codex","worker":worker,"turn":"t","pid":null,"process_stamp":null})).unwrap();
    let error = std::env::var("KAH_SERVICE_ERROR").unwrap();
    let legacy = json!({"request":"request-402","error":error,"origin":"report_validation"});
    kah_cli::unattended_report::save(&root, "gap.json", &legacy).unwrap();
    drop(store);
    queue::enqueue(
        worker,
        Purpose::Checkpoint,
        "later",
        json!({"events":[{"kind":"user_gate"}]}),
    )
    .unwrap();
    let before = queue::entries(worker)
        .unwrap()
        .iter()
        .map(|(p, _)| (p.clone(), std::fs::read(p).unwrap()))
        .collect::<Vec<_>>();
    assert!(service::blocked(worker).unwrap());
    assert!(availability::unavailable(&home).unwrap());
    // Changing transient symptoms and queued requests must not send another notice.
    if !error.contains("402") {
        availability::record_error(&home, "JEV_HTTP_TIMEOUT stage=judge").unwrap();
    }
    assert!(service::blocked(worker).unwrap());
    assert_eq!(
        queue::drain(worker).unwrap()["status"],
        "service_unavailable"
    );
    assert_eq!(reminder_queue::pending(&root).unwrap().len(), 1);
    assert_one_notice(worker, &root, &legacy);
    assert!(!root.join("redelivery.json").exists());
    for (path, bytes) in before {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(
        Store::read(worker).unwrap().unwrap().pending.unwrap().id,
        "request-402"
    );
    availability::record(&home, None).unwrap();
    assert_recovery(worker, &root, &legacy);
}
fn assert_recovery(worker: &str, root: &std::path::Path, legacy: &serde_json::Value) {
    use kah_cli::observer_review::{service, state::Store};
    assert!(!service::blocked(worker).unwrap());
    assert!(!root.join("gap.json").exists());
    let recovered: serde_json::Value =
        kah_cli::observer_review::state::read(&root.join("service-outage.json"))
            .unwrap()
            .unwrap();
    assert_eq!(&recovered["previous"]["original_gap"], legacy);
    assert_eq!(
        Store::read(worker).unwrap().unwrap().pending.unwrap().id,
        "request-402"
    );
}

fn assert_one_notice(worker: &str, root: &std::path::Path, legacy: &serde_json::Value) {
    let notice = kah_cli::observer_review::consume::take(worker, "PostToolUse", "t")
        .unwrap()
        .unwrap();
    assert!(
        notice["systemMessage"]
            .as_str()
            .unwrap()
            .contains("Jev unavailable")
    );
    assert!(
        kah_cli::observer_review::consume::take(worker, "PostToolUse", "t")
            .unwrap()
            .is_none()
    );
    // A later raw failure timestamp is retained, but cannot create a second notice.
    kah_cli::unattended_report::save(root, "gap.json", legacy).unwrap();
    assert!(
        kah_cli::observer_review::consume::take(worker, "PostToolUse", "t")
            .unwrap()
            .is_none()
    );
}
