#[path = "../src/privacy.rs"]
mod privacy;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[allow(dead_code)]
mod track2_support;

#[test]
#[ignore = "ten real builder commits through the full native C observer; make gate only"]
fn real_commit_cache_benchmark() {
    track2_support::launch("cache");
}

#[test]
#[ignore = "frozen detour/integration/deferral native proofs on both providers; final code only"]
fn detour_integration_deferral() {
    track2_support::launch("proofs");
}

#[test]
#[ignore = "full native retained-session journey on both providers; make gate only"]
fn long_session() {
    track2_support::launch("long");
}

#[test]
#[ignore = "same native observer through three real checkpoints on both providers"]
fn three_checkpoint_memory() {
    track2_support::launch("memory");
}

#[test]
fn private_entrypoint() {
    if let Ok(mode) = std::env::var("KAH_TRACK2_ENTRYPOINT") {
        track2_support::entrypoint(&mode);
    }
}
#[test]
fn fixture_writer() {
    if let Ok(worker) = std::env::var("KAH_TRACK2_WRITER") {
        track2_support::write_run_store(&worker);
    }
}
