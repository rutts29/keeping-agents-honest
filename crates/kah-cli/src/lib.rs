//! Native agent launcher with project-scoped trust policy.
mod arguments;
mod launch;
pub use arguments::Arguments;
pub use launch::Launch;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
