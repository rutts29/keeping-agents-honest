//! git/git v2.53.0 Documentation/git-diff.adoc --no-ext-diff/--no-textconv;
//! existing commit_context uses the same isolated read-only Git invocation.
use super::*;
use kah_observer::commit_check::{CodeFile, Hunk};
use std::process::Command;
pub fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "MEASURE_GIT stage=source: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    String::from_utf8(output.stdout).map_err(Into::into)
}
pub fn collect(
    root: &Path,
    baseline: &str,
    head: &str,
) -> Result<(BTreeMap<String, Hunk>, BTreeMap<String, CodeFile>)> {
    let names = git(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            baseline,
            head,
            "--",
        ],
    )?;
    let mut hunks = BTreeMap::new();
    let mut files = BTreeMap::new();
    for path in names.split('\0').filter(|p| !p.is_empty()) {
        let patch = git(
            root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--binary",
                baseline,
                head,
                "--",
                path,
            ],
        )?;
        hunks.insert(
            commit_check::key(path),
            Hunk {
                file: path.into(),
                text: patch,
            },
        );
        if !git(root, &["ls-tree", "-z", head, "--", path])?.is_empty() {
            let spec = format!("{head}:{path}");
            let text = git(root, &["show", &spec])?;
            files.insert(path.into(), CodeFile::new(head, &text));
        }
    }
    Ok((hunks, files))
}
