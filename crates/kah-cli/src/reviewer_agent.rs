//! Herdr agent name that kah sends supervision alerts to.
use std::sync::OnceLock;

pub const DEFAULT: &str = "kah-reviewer";
pub const ENV: &str = "KAH_REVIEWER_AGENT";
const LIMIT: usize = 32;

/// Resolve a configured name. `Err` carries the rejected value.
pub fn resolve(value: Option<&str>) -> Result<&str, &str> {
    match value {
        None | Some("") => Ok(DEFAULT),
        Some(name)
            if name.len() <= LIMIT
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && name.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                }) =>
        {
            Ok(name)
        }
        Some(name) => Err(name),
    }
}

/// Agent name for alerts: `KAH_REVIEWER_AGENT` when valid, else `kah-reviewer`.
pub fn reviewer_agent() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        let raw = std::env::var(ENV).ok();
        match resolve(raw.as_deref()) {
            Ok(name) => name.to_string(),
            Err(bad) => {
                eprintln!("kah: {ENV} stage=reviewer_agent: invalid name {bad:?}; using {DEFAULT}");
                DEFAULT.to_string()
            }
        }
    })
}
