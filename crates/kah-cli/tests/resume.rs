use kah_cli::{Arguments, resume::apply_location};
use std::path::Path;

#[test]
fn recorded_overlay_restores_prior_network_choice_without_global_settings() {
    let mut args = Arguments::parse(vec!["--".into(), "resume".into(), "id".into()]).unwrap();
    apply_location(
        Path::new("/home/operator"),
        Path::new("/home/operator/.local/state/kah/codex/research-online/hash/sessions/file"),
        &mut args,
    );
    assert!(args.options.online);
    assert!(!args.options.yolo);
}

#[test]
fn unrelated_profile_paths_cannot_supply_an_kah_policy() {
    let mut args = Arguments::default();
    apply_location(
        Path::new("/home/operator"),
        Path::new("/tmp/research-online/sessions/file"),
        &mut args,
    );
    assert!(!args.options.online);
    assert!(!args.options.yolo);
}

#[test]
fn untrusted_resume_restores_the_explicit_online_policy() {
    let mut args = Arguments::default();
    apply_location(
        Path::new("/home/operator"),
        Path::new("/home/operator/.local/state/kah/codex/untrusted-online/hash/sessions/file"),
        &mut args,
    );
    assert!(args.options.online);
    assert!(!args.options.yolo);
}
