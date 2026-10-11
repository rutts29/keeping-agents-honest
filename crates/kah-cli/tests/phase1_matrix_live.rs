mod phase1_matrix_support;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
#[test]
#[ignore = "ONE full native persistent matrix session; make gate only; no Jev"]
fn persistent_native_matrices() {
    phase1_matrix_support::launch();
}
#[test]
#[ignore = "reviewer-authorized real Jev confirmation in SAME native session; make gate only"]
fn confirm_real_jev() {
    phase1_matrix_support::confirmation_replay();
}
#[test]
fn matrix_entrypoint() {
    if std::env::var_os("KAH_PHASE1_ROOT").is_some() {
        phase1_matrix_support::entry();
    }
}
