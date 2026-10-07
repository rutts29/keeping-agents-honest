use kah_observer::judge::availability;
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
