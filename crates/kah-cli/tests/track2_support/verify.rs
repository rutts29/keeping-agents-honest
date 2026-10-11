#[path = "bundle_cost.rs"]
mod bundle_cost;
#[path = "claude_invocation.rs"]
mod claude_invocation;
#[path = "claude_login.rs"]
mod claude_login;
#[path = "claude_metadata.rs"]
mod claude_metadata;
#[path = "claude_trust.rs"]
mod claude_trust;
#[path = "config_audit.rs"]
mod config_audit;
pub fn claude_login_ready() {
    claude_login::require_ready();
}
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
