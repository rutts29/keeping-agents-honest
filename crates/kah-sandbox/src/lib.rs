//! Directory policy and rootless executor lifecycle.

mod executor;
mod initialize;
mod policy;
mod project;
mod runtime;
pub use executor::{Executor, output_retrying_start, sandbox_start_failed};
pub use policy::{Domain, Error, LaunchOptions};
pub use project::Project;
pub use runtime::{current_uid, prepare_public_workspace, publish_binary};

mod setup;
pub use setup::{global_setup_mounts, global_setup_mounts_with_claude};

pub mod git_guard;
