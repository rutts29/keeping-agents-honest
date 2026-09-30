//! Native agent launcher with project-scoped trust policy.
mod arguments;
pub mod herdr;
pub mod inspection_cli;
mod launch;
pub mod observer_hook;
pub mod resume;
pub mod session_policy;
pub use arguments::Arguments;
pub use launch::Launch;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub mod attached_observer;
pub mod evaluation_cli;
pub mod fetch_cli;
pub mod review_cli;
pub mod run_commands;
pub mod send_cli;
pub mod unattended_args;
pub mod unattended_observer;
pub mod unattended_report;
pub mod unattended_run;
pub mod unattended_setup;

pub mod shell_routing;
pub mod smoke;
