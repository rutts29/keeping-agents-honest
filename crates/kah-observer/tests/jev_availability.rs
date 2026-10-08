use kah_observer::judge::availability;
#[test]
fn recovery_backs_off_without_native_work_and_success_releases_outage() {
    use kah_observer::judge::Error;
    let home = std::env::temp_dir().join(format!("kah-jev-recovery-{}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    availability::record_error(&home, "JEV_HTTP_STATUS stage=judge http_status=503").unwrap();
    let start = availability::snapshot(&home).unwrap().unwrap()["observed_ms"]
        .as_u64()
        .unwrap();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(home.join(".local/state/kah/jev-service.lock"))
        .unwrap();
    lock.lock().unwrap();
    availability::recovery::with_probe(&home, start + 15_000, || panic!("concurrent probe"))
        .unwrap();
    drop(lock);
    availability::recovery::with_probe(&home, start, || panic!("probe ignored backoff")).unwrap();
    availability::recovery::with_probe(&home, start + 15_000, || {
        Err(Error::new("JEV_HTTP_TIMEOUT"))
    })
    .unwrap();
    assert!(availability::unavailable(&home).unwrap());
    availability::recovery::with_probe(&home, start + 15_001, || panic!("duplicate probe"))
        .unwrap();
    availability::recovery::with_probe(&home, start + 30_000, || {
        Err(Error::new("JEV_HTTP_TIMEOUT"))
    })
    .unwrap();
    availability::record_error(&home, "JEV_HTTP_STATUS stage=judge http_status=529").unwrap();
    availability::recovery::with_probe(&home, start + 30_001, || {
        panic!("probe ignored increasing backoff")
    })
    .unwrap();
    availability::recovery::with_probe(&home, start + 60_000, || Ok(())).unwrap();
    assert!(!availability::unavailable(&home).unwrap());
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn transient_outages_block_admission_and_keep_one_epoch_until_success() {
    let home = std::env::temp_dir().join(format!("kah-jev-transient-{}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    for error in [
        "JEV_HTTP_TIMEOUT stage=judge",
        "JEV_HTTP_STATUS stage=judge http_status=503",
        "JEV_HTTP_STATUS stage=judge http_status=529",
    ] {
        availability::record_error(&home, error).unwrap();
        assert!(availability::unavailable(&home).unwrap());
    }
    let epoch = availability::snapshot(&home).unwrap().unwrap()["observed_ms"].clone();
    availability::record_error(&home, "JEV_HTTP_TIMEOUT stage=judge").unwrap();
    assert_eq!(
        availability::snapshot(&home).unwrap().unwrap()["observed_ms"],
        epoch
    );
    assert!(!availability::outage(
        "JEV_CONTEXT_LIMIT stage=judge http_status=400"
    ));
    availability::record(&home, None).unwrap();
    assert!(!availability::unavailable(&home).unwrap());
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn payment_outage_persists_until_a_successful_judgment_without_a_poll_or_deadline() {
    let home = std::env::temp_dir().join(format!("kah-jev-service-{}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    assert!(!availability::unavailable(&home).unwrap());
    availability::record(&home, Some(402)).unwrap();
    assert!(availability::unavailable(&home).unwrap());
    let epoch = availability::snapshot(&home).unwrap();
    availability::record(&home, Some(402)).unwrap();
    assert_eq!(availability::snapshot(&home).unwrap(), epoch);
    availability::record(&home, None).unwrap();
    assert!(!availability::unavailable(&home).unwrap());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn first_success_records_available_even_before_a_legacy_outage_is_encountered() {
    let home = std::env::temp_dir().join(format!("kah-jev-first-success-{}", std::process::id()));
    std::fs::create_dir(&home).unwrap();
    availability::record(&home, None).unwrap();
    assert!(availability::snapshot(&home).unwrap().is_some());
    assert!(!availability::unavailable(&home).unwrap());
    std::fs::remove_dir_all(home).unwrap();
}
