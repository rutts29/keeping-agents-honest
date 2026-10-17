//! Worker Git metadata is untrusted. Execute it only inside the sandbox and
//! transfer objects through a bundle into a separate, controller-owned repo.
use super::{Worktree, command, io_error, run, text};
use kah_sandbox::Executor;
use std::{fs, os::unix::fs::OpenOptionsExt, path::Path, process::Command};

impl Worktree {
    /// Start from an object-only bundle, without reading source configuration
    /// on the host. Uses the same fsck/expected-SHA import as run delivery.
    pub fn from_bundle(
        source: &Path,
        directory: &Path,
        bundle: &Path,
        expected: &str,
        id: &str,
    ) -> Result<Self, String> {
        validate_commit(expected)?;
        if !id.starts_with("run-")
            || id.len() > 80
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("RUN_ID stage=worktree".into());
        }
        fs::create_dir(directory).map_err(io_error)?;
        let directory = directory.canonicalize().map_err(io_error)?;
        let git_dir = directory.join("repo.git");
        let path = directory.join("worktree");
        run(command()
            .args(["init", "--quiet", "--template=", "--separate-git-dir"])
            .arg(&git_dir)
            .arg(&path))?;
        let tree = Self {
            path,
            admin: git_dir.clone(),
            git_dir,
            base_commit: expected.into(),
            source: source.canonicalize().map_err(io_error)?,
            branch: format!("kah/{id}"),
        };
        run(tree.git().args([
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{}", tree.branch),
        ]))?;
        tree.import_bundle(bundle, expected)?;
        Ok(tree)
    }

    pub fn sandbox_state(&self, executor: &Executor) -> Result<(String, bool), String> {
        let head = text(run(self.sandbox_git(executor)?.args([
            "rev-parse",
            "--verify",
            "HEAD^{commit}",
        ]))?)?;
        validate_commit(&head)?;
        let dirty = !run(self.sandbox_git(executor)?.args([
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ]))?
        .stdout
        .is_empty();
        Ok((head, dirty))
    }
    pub fn bundle_head(&self, executor: &Executor, bundle: &Path) -> Result<String, String> {
        let commit = text(run(self.sandbox_git(executor)?.args([
            "rev-parse",
            "--verify",
            "HEAD^{commit}",
        ]))?)?;
        validate_commit(&commit)?;
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(bundle)
            .map_err(io_error)?;
        let mut create = self.sandbox_git(executor)?;
        create.args(["bundle", "create", "-", "HEAD"]).stdout(file);
        // A start failure writes nothing, so the retry fills the same new file.
        let result = kah_sandbox::output_retrying_start(&mut create).map_err(io_error)?;
        if !result.status.success() {
            let _ = fs::remove_file(bundle);
            return Err(format!(
                "RUN_BUNDLE_CREATE stage=checkpoint: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        Ok(commit)
    }

    /// Call only on the trusted candidate, never the live worker's Git store.
    pub fn import_bundle(&self, bundle: &Path, expected: &str) -> Result<(), String> {
        validate_commit(expected)?;
        if !fs::symlink_metadata(bundle).map_err(io_error)?.is_file() {
            return Err(
                "RUN_BUNDLE_TYPE stage=checkpoint: expected controller-owned regular file".into(),
            );
        }
        run(self
            .git()
            .args([
                "-c",
                "fetch.fsckObjects=true",
                "-c",
                "transfer.fsckObjects=true",
                "fetch",
                "--quiet",
                "--force",
                "--no-tags",
                "--no-recurse-submodules",
                "--no-write-fetch-head",
            ])
            .arg(bundle)
            .arg("HEAD:refs/kah/import"))?;
        let actual = text(run(self.git().args([
            "rev-parse",
            "--verify",
            "refs/kah/import^{commit}",
        ]))?)?;
        if actual != expected {
            return Err("RUN_BUNDLE_COMMIT_CHANGED stage=checkpoint: retry after the native session settles".into());
        }
        run(self.git().args(["reset", "--hard", "--quiet", expected]))?;
        Ok(())
    }

    fn sandbox_git(&self, executor: &Executor) -> Result<Command, String> {
        let template = self.git();
        let mut args = vec!["/usr/bin/env".to_owned(), "-i".into()];
        for (key, value) in template
            .get_envs()
            .filter_map(|(key, value)| value.map(|v| (key, v)))
        {
            args.push(format!(
                "{}={}",
                key.to_str().ok_or("RUN_GIT_UTF8 stage=checkpoint")?,
                value.to_str().ok_or("RUN_GIT_UTF8 stage=checkpoint")?
            ));
        }
        args.push("/usr/bin/git".into());
        for arg in template.get_args() {
            args.push(arg.to_str().ok_or("RUN_GIT_UTF8 stage=checkpoint")?.into());
        }
        args.extend([
            "-c".into(),
            format!("safe.directory={}", self.path.display()),
        ]);
        let mut command = executor
            .command(&self.path, &args)
            .map_err(|e| e.to_string())?;
        command.stdin(std::process::Stdio::null());
        Ok(command)
    }
}
fn validate_commit(commit: &str) -> Result<(), String> {
    if !matches!(commit.len(), 40 | 64) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("RUN_COMMIT_ID stage=checkpoint".into());
    }
    Ok(())
}
