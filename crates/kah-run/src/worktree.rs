//! A private Git repository backs the run worktree. Mount its metadata read-only
//! in gVisor; the controller owns checkpoints and exports only the run ref.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worktree {
    pub path: PathBuf,
    pub git_dir: PathBuf,
    pub base_commit: String,
    source: PathBuf,
    admin: PathBuf,
    branch: String,
}
impl Worktree {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(io_error)?;
        file.write_all(
            &serde_json::to_vec(self)
                .map_err(|e| format!("RUN_WORKTREE_JSON stage=worktree: {e}"))?,
        )
        .and_then(|()| file.sync_all())
        .map_err(io_error)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        use std::io::Read;
        if !fs::symlink_metadata(path).map_err(io_error)?.is_file() {
            return Err("RUN_WORKTREE_STATE_TYPE stage=resume".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(io_error)?
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > 65536 {
            return Err("RUN_WORKTREE_STATE_BOUND stage=resume".into());
        }
        let tree: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("RUN_WORKTREE_JSON stage=resume: {e}"))?;
        if !tree.path.is_absolute()
            || !tree.git_dir.is_absolute()
            || !tree.source.is_absolute()
            || tree.path.parent() != tree.git_dir.parent()
            || !tree.admin.starts_with(&tree.git_dir)
            || tree.git_dir.canonicalize().map_err(io_error)? != tree.git_dir
            || !tree.branch.starts_with("kah/run-")
        {
            return Err("RUN_WORKTREE_BINDING stage=resume".into());
        }
        Ok(tree)
    }

    pub fn create(source: &Path, directory: &Path, id: &str) -> Result<Self, String> {
        if !id.starts_with("run-")
            || id.len() > 80
            || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err("RUN_ID stage=worktree".into());
        }
        let source = source.canonicalize().map_err(io_error)?;
        validate_source(&source)?;
        fs::create_dir(directory)
            .map_err(|e| format!("RUN_DIRECTORY_CREATE stage=worktree: {e}"))?;
        let directory = directory.canonicalize().map_err(io_error)?;
        let git_dir = directory.join("repo.git");
        let path = directory.join("worktree");
        let base_commit = text(run(command().arg("-C").arg(&source).args([
            "rev-parse",
            "--verify",
            "HEAD^{commit}",
        ]))?)?;
        clone_metadata(&source, &git_dir)?;
        let branch = format!("kah/{id}");
        run(command()
            .arg("--git-dir")
            .arg(&git_dir)
            .args([
                "worktree",
                "add",
                "--quiet",
                "--no-guess-remote",
                "-b",
                &branch,
            ])
            .arg(&path)
            .arg(&base_commit))?;
        let admin = PathBuf::from(text(run(command()
            .arg("-C")
            .arg(&path)
            .args(["rev-parse", "--absolute-git-dir"]))?)?);
        if !admin.starts_with(&git_dir) {
            return Err("RUN_GIT_DIRECTORY stage=worktree".into());
        }
        Ok(Self {
            path,
            git_dir,
            base_commit,
            source,
            admin,
            branch,
        })
    }
    pub fn checkpoint(&self, message: &str) -> Result<String, String> {
        run(self.git().args(["add", "--all", "--", "."]))?;
        let changed = self
            .git()
            .args(["diff", "--cached", "--quiet", "--exit-code"])
            .status()
            .map_err(io_error)?;
        match changed.code() {
            Some(0) => (),
            Some(1) => {
                run(self
                    .git()
                    .args(["commit", "--quiet", "--no-gpg-sign", "-m", message]))?;
            }
            _ => return Err("RUN_INDEX_DIFF stage=checkpoint".into()),
        }
        text(run(self.git().args(["rev-parse", "HEAD"]))?)
    }
    pub fn export_branch(&self) -> Result<(), String> {
        // Local ref transfer only. No remote, credentials, deploy or push.
        let reference = format!("refs/heads/{}:refs/heads/{}", self.branch, self.branch);
        run(command()
            .arg("-C")
            .arg(&self.source)
            .args([
                "fetch",
                "--quiet",
                "--no-tags",
                "--no-write-fetch-head",
                "--no-recurse-submodules",
            ])
            .arg(&self.git_dir)
            .arg(reference))?;
        Ok(())
    }
    pub fn diff_stat(&self) -> Result<String, String> {
        text(run(self.git().args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--stat",
            &self.base_commit,
            "HEAD",
        ]))?)
    }
    pub fn restore_checkpoint(&self, commit: &str) -> Result<(), String> {
        if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("RUN_COMMIT_ID stage=worktree_restore".into());
        }
        run(self.git().args(["reset", "--hard", commit]))?;
        run(self.git().args(["clean", "-fd", "--", "."]))?;
        // Reassert the trusted pointer even if an interrupted worker replaced it.
        let pointer = self.path.join(".git");
        fs::remove_file(&pointer).map_err(io_error)?;
        fs::write(pointer, format!("gitdir: {}\n", self.admin.display())).map_err(io_error)
    }
    pub fn diff_evidence(&self, limit: usize) -> Result<String, String> {
        use std::{io::Read, process::Stdio};
        let mut child = self
            .git()
            .args([
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--full-index",
                &self.base_commit,
                "HEAD",
                "--",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(io_error)?;
        let mut bytes = Vec::new();
        let read = child
            .stdout
            .take()
            .unwrap()
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes);
        if read.is_err() || bytes.len() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("RUN_DIFF_BOUND stage=completion: diff exceeds evidence bound".into());
        }
        if !child.wait().map_err(io_error)?.success() {
            return Err("RUN_DIFF_READ stage=completion".into());
        }
        String::from_utf8(bytes).map_err(|_| "RUN_DIFF_UTF8 stage=completion".into())
    }
    fn git(&self) -> Command {
        let mut cmd = command();
        cmd.current_dir(&self.path)
            .arg("--git-dir")
            .arg(&self.admin)
            .arg("--work-tree")
            .arg(&self.path);
        cmd
    }
}

fn validate_source(source: &Path) -> Result<(), String> {
    let git = source.join(".git");
    if !fs::symlink_metadata(&git).map_err(io_error)?.is_dir() {
        return Err(
            "RUN_SOURCE_GIT_LAYOUT stage=worktree: source needs an ordinary repository directory"
                .into(),
        );
    }
    let alternates = git.join("objects/info/alternates");
    if fs::read(&alternates).is_ok_and(|bytes| !bytes.is_empty()) {
        return Err(
            "RUN_SOURCE_ALTERNATES stage=worktree: shared external object stores are not accepted"
                .into(),
        );
    }
    Ok(())
}
fn command() -> Command {
    let mut cmd = Command::new("/usr/bin/git");
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "kah")
        .env("GIT_AUTHOR_EMAIL", "kah@localhost")
        .env("GIT_COMMITTER_NAME", "kah")
        .env("GIT_COMMITTER_EMAIL", "kah@localhost");
    for setting in [
        "core.hooksPath=/dev/null",
        "core.fsmonitor=false",
        "core.attributesFile=/dev/null",
        "commit.gpgsign=false",
        "gc.auto=0",
        "maintenance.auto=false",
        "protocol.file.allow=always",
        "protocol.ext.allow=never",
    ] {
        cmd.args(["-c", setting]);
    }
    cmd
}
fn run(cmd: &mut Command) -> Result<Output, String> {
    let output = cmd.output().map_err(io_error)?;
    if !output.status.success() {
        return Err(format!(
            "RUN_GIT stage=worktree: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output)
}
fn text(output: Output) -> Result<String, String> {
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| "RUN_GIT_TEXT stage=worktree: output is not UTF-8".into())
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_WORKTREE_IO stage=worktree: {error}")
}

fn clone_metadata(source: &Path, git_dir: &Path) -> Result<(), String> {
    run(command()
        .args([
            "clone",
            "--quiet",
            "--local",
            "--no-hardlinks",
            "--bare",
            "--single-branch",
            "--no-tags",
            "--template=",
        ])
        .arg(source)
        .arg(git_dir))?;
    // Bare clones can have no remote section depending on Git version.
    let mut remove = command();
    remove
        .arg("--git-dir")
        .arg(git_dir)
        .args(["config", "--remove-section", "remote.origin"]);
    let removed = remove.output().map_err(io_error)?;
    if !removed.status.success() && removed.status.code() != Some(128) {
        return Err("RUN_REMOTE_REMOVE stage=worktree".into());
    }
    Ok(())
}
