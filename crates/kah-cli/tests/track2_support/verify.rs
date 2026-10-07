#[path = "config_audit.rs"]
mod config_audit;
#[path = "fixture.rs"]
mod fixture;
#[path = "launcher.rs"]
mod launcher;
#[path = "proof_inputs.rs"]
mod proof_inputs;
#[path = "trust.rs"]
mod trust;
pub use launcher::launch;
pub use trust::{consent as private_consent, preseed_claude};
