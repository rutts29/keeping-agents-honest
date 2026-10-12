//! Codex rust-v0.162.1 app-server-protocol/src/protocol/v2/plugin.rs:468
//! reports the declaring SKILL.md path; an upstream checkout layout is kah's
//! provenance convention. Isolated copies must prove whole source identity.
use std::{fs, io, path::Path};
pub fn matches(path: &Path, name: &str, seed: Option<&Path>, home: &Path) -> io::Result<bool> {
    let Some(seed) = seed else {
        return Ok(path.to_string_lossy().contains("/agent-skills/sources/"));
    };
    let path = path.canonicalize()?;
    if !path.starts_with(home.canonicalize()?) {
        return Ok(false);
    }
    Ok(fs::read(path)? == fs::read(seed.join(name).join("SKILL.md"))?)
}
