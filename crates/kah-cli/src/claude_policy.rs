//! Native file permissions protect private host state without hiding installed code.
use crate::Result;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn permissions(
    home: &Path,
    project: &Path,
    private: &Path,
    online: bool,
    git: Option<&Path>,
) -> Result<Value> {
    let setup =
        kah_sandbox::global_setup_mounts_with_claude(home, &kah_claude::paths::config_root(home))?;
    let mut allow = vec![
        "Bash".into(),
        "WebSearch".into(),
        format!("Edit(/{}/**)", project.display()),
    ];
    for (source, target) in &setup {
        rules(&mut allow, "Read", source);
        if source != target {
            rules(&mut allow, "Read", target);
        }
    }
    let mut protected = vec![
        home.join(".ssh"),
        home.join(".config"),
        home.join(".claude.json"),
        kah_claude::paths::global_config(home),
        home.join(".git-credentials"),
        home.join(".netrc"),
        home.join(".npmrc"),
        home.join(".local/state/kah"),
        home.join(".local/share/kah-eval"),
        private.into(),
    ];
    // The active root, when XDG moves it; earlier state stays at the default.
    let state = kah_core::run_store::RunStore::state_root_for(home);
    if !protected.contains(&state) {
        protected.push(state);
    }
    for root in [
        home.join(".codex"),
        home.join(".claude"),
        kah_claude::paths::config_root(home),
    ] {
        for path in children(&root)? {
            if !setup.iter().any(|(_, target)| target == &path) {
                protected.push(path);
            }
        }
    }
    for domain in ["mine", "research", "untrusted"] {
        for path in children(&home.join("repos").join(domain))? {
            if path.canonicalize().ok().as_deref() != Some(project) {
                protected.push(path);
            }
        }
    }
    let mut deny = vec![];
    // Git remains native in Bash, but host file tools must not bypass the
    // sandbox's read-only Git control mounts (including linked worktree pointers).
    rules(&mut deny, "Edit", &project.join(".git"));
    rules(&mut deny, "Edit", &home.join(".gitconfig"));
    if let Some(git) = git {
        rules(&mut deny, "Edit", git);
        for tool in ["Read", "Edit"] {
            rules(
                &mut deny,
                tool,
                &git.parent()
                    .ok_or("CLAUDE_GIT_PATH stage=permissions")?
                    .join("candidate"),
            );
        }
    }
    for path in protected {
        for tool in ["Read", "Edit"] {
            rules(&mut deny, tool, &path);
        }
    }
    for (_, path) in setup {
        if !path.starts_with(project) {
            rules(&mut deny, "Edit", &path);
        }
    }
    if !online {
        deny.push("WebFetch".into());
    }
    Ok(json!({"allow":allow,"deny":deny}))
}
fn children(path: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .map(|entry| {
                entry
                    .map(|e| e.path())
                    .map_err(|e| format!("CLAUDE_POLICY_READ stage=permissions: {e}").into())
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("CLAUDE_POLICY_READ stage=permissions: {e}").into()),
    }
}
fn rules(output: &mut Vec<String>, tool: &str, path: &Path) {
    output.push(format!("{tool}(/{})", path.display()));
    output.push(format!("{tool}(/{}/**)", path.display()));
}
