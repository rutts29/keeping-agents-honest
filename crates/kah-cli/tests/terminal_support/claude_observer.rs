#![allow(dead_code)]
//! kah's Claude observer choice for live tests. Workers keep their own fixture
//! model/effort; only the observer kah launches for them uses this setting
//! (`--observer-model` / `--observer-effort`), shared by every live Claude launch.
use serde_json::{Value, json};

pub const MODEL: &str = "claude-haiku-5-5";
pub const EFFORT: &str = "high";

pub fn model() -> String {
    std::env::var("KAH_GATE_CLAUDE_OBSERVER_MODEL").unwrap_or_else(|_| MODEL.into())
}
pub fn effort() -> String {
    std::env::var("KAH_GATE_CLAUDE_OBSERVER_EFFORT").unwrap_or_else(|_| EFFORT.into())
}
/// Set the observer choice on parsed launch arguments (Claude workers only).
pub fn apply(args: &mut kah_cli::Arguments) {
    args.observer_model = Some(model());
    args.observer_effort = Some(effort());
}
/// Arguments for an `kah run` Claude launch.
pub fn run_args() -> Vec<String> {
    vec![
        "--observer-model".into(),
        model(),
        "--observer-effort".into(),
        effort(),
    ]
}
/// The same arguments for a shell command line.
pub fn shell() -> String {
    format!(
        "--observer-model '{}' --observer-effort '{}'",
        model().replace('\'', "'\\''"),
        effort().replace('\'', "'\\''")
    )
}
/// Evidence field naming the observer choice a Claude journey ran with.
pub fn evidence() -> Value {
    json!({"model": model(), "effort": effort()})
}
