mod live_support;

#[test]
#[ignore = "live installed Codex; run only at a gate"]
fn project_trust_persistence_is_confined_to_overlay() {
    live_support::trust::verify();
}

#[test]
#[ignore = "live native startup and model turn; run only at a gate"]
fn restricted_overlay_blocks_host_extensions() {
    live_support::extensions::verify();
}
