//! Native agent launcher with project-scoped trust policy.
mod arguments;
pub mod claude_hook;
pub mod claude_launch;
pub mod claude_policy;
pub mod claude_setup;
pub mod completion_capture;
pub mod completion_inputs;
pub mod completion_replay;
pub mod completion_review;
pub mod dispatch;
pub mod herdr;
pub mod in_turn;
pub mod in_turn_worker;
pub mod inspection_cli;
pub mod interactive_name;
mod launch;
pub mod observer_hook;
pub mod observer_notice;
pub mod observer_review;
mod privacy;
pub mod relay;
pub mod relay_hook;
pub mod relay_store;
pub mod relay_transport;
pub mod resume;
pub mod session_policy;
pub use arguments::Arguments;
pub use launch::Launch;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub mod attached_observer;
pub mod evaluation_cli;
pub mod review_cli;
pub mod run_commands;
pub mod send_cli;
pub mod unattended_args;
pub mod unattended_observer;
pub mod unattended_report;
pub mod unattended_run;
pub mod unattended_setup;
pub mod user_steer;

pub mod native_setup;
pub mod sandbox_command;
pub mod setup_policy;
pub mod shell_routing;
pub mod smoke;

pub mod native_run;

pub mod native_binding;
pub mod native_child;
pub mod native_controller;
pub mod native_history;
pub mod native_resume;
pub mod run_pane;
