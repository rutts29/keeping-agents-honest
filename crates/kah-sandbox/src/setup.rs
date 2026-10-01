//! Direct mounts of the user's installed setup, without snapshots or parent homes.
use crate::Error;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn global_setup_mounts(home: &Path) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
    let mut mounts = BTreeMap::new();
    for relative in [
        ".agents/skills",
        ".codex/skills",
        ".codex/plugins",
        ".codex/packages",
        ".codex/rules",
        ".codex/hooks",
        ".codex/herdr-agent-state.sh",
        ".codex/agents",
        ".claude/skills",
        ".claude/plugins",
        ".claude/commands",
        ".claude/agents",
        ".claude/rules",
        ".claude/hooks",
        ".claude/statusline.py",
        ".local/share/agent-skills",
        ".codex/AGENTS.md",
        ".codex/AGENTS.override.md",
        ".claude/CLAUDE.md",
    ] {
        let target = home.join(relative);
        if !target.try_exists().map_err(io_error)? {
            continue;
        }
        let source = target.canonicalize().map_err(io_error)?;
        mounts.insert(target, source);
    }
    // Installed skill aliases may point to a package outside these roots. Mount
    // that exact skill directory too, not its parent repository or home.
    for relative in [".agents/skills", ".codex/skills", ".claude/skills"] {
        let directory = home.join(relative);
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.is_symlink() && path.join("SKILL.md").is_file() {
                let source = path.canonicalize().map_err(io_error)?;
                if !mounts.keys().any(|target| source.starts_with(target)) {
                    mounts.insert(source.clone(), source);
                }
            }
        }
    }
    Ok(mounts
        .into_iter()
        .map(|(target, source)| (source, target))
        .collect())
}

fn io_error(error: std::io::Error) -> Error {
    Error::new("SETUP_IO", "global_setup", error)
}
