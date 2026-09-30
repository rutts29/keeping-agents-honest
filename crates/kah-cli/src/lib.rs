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
pub mod run_commands;
pub mod unattended_args;
pub mod unattended_observer;
pub mod unattended_setup;
