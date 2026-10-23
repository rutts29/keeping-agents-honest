mod live_support;

#[test]
#[ignore = "live test; requires installed Codex and a native login; run manually"]
fn project_trust_persistence_is_confined_to_overlay() {
    live_support::trust::verify();
}

#[test]
#[ignore = "live native startup and model turn; run manually"]
fn restricted_overlay_blocks_host_extensions() {
    live_support::extensions::verify();
}
