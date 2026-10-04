#[test]
fn new_and_retargeted_tags_trigger_but_existing_tags_do_not() {
    let before = vec!["refs/tags/phase-1 abc".into()];
    assert!(kah_cli::observer_review::tags::additions(&before, &before).is_empty());
    let after = vec![
        "refs/tags/phase-1 def".into(),
        "refs/tags/phase-2 ghi".into(),
    ];
    assert_eq!(
        kah_cli::observer_review::tags::additions(&before, &after),
        after
    );
}
