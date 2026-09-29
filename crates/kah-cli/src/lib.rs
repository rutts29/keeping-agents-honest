//! Native agent launcher with project-scoped trust policy.
mod arguments;
pub mod herdr;
mod launch;
pub mod resume;
pub use arguments::Arguments;
pub use launch::Launch;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
