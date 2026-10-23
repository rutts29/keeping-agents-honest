use super::{Worktree, io_error};
use serde_json::{Value, json};
use std::{
    io::Read,
    process::{Command, Stdio},
};

/// Host memory the completion evidence may take from one git output: the
/// diff, the changed-file contents (together) and each path inventory.
pub const EVIDENCE_LIMIT: usize = 4 * 1024 * 1024;

/// Standard output of a git child, or None when git fails. At most `limit`
/// bytes are held: a larger output stops the child and is refused, so a large
/// committed file is never read whole into the host.
fn collect(command: &mut Command, limit: usize, what: &str) -> Result<Option<Vec<u8>>, String> {
    let mut child = command
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
        return Err(if read.is_err() {
            format!("RUN_EVIDENCE_READ stage=completion: {what}")
        } else {
            format!(
                "RUN_EVIDENCE_LIMIT stage=completion: {what} is larger than {limit} bytes; the candidate stays in the run directory"
            )
        });
    }
    Ok(child.wait().map_err(io_error)?.success().then_some(bytes))
}

fn paths(bytes: &[u8], reason: &'static str) -> Result<Vec<String>, String> {
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8(p.to_vec()).map_err(|_| format!("{reason} stage=completion")))
        .collect()
}

impl Worktree {
    pub fn diff_evidence(&self) -> Result<String, String> {
        self.diff_evidence_within(EVIDENCE_LIMIT)
    }
    pub fn diff_evidence_within(&self, limit: usize) -> Result<String, String> {
        let bytes = collect(
            self.git().args([
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--full-index",
                &self.base_commit,
                "HEAD",
                "--",
            ]),
            limit,
            "the candidate diff",
        )?
        .ok_or("RUN_DIFF_READ stage=completion")?;
        String::from_utf8(bytes).map_err(|_| "RUN_DIFF_UTF8 stage=completion".into())
    }
    pub fn changed_paths(&self) -> Result<Vec<String>, String> {
        self.changed_paths_within(EVIDENCE_LIMIT)
    }
    pub fn changed_paths_within(&self, limit: usize) -> Result<Vec<String>, String> {
        let bytes = collect(
            self.git().args([
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--name-only",
                "-z",
                &self.base_commit,
                "HEAD",
                "--",
            ]),
            limit,
            "the changed path list",
        )?
        .ok_or("RUN_GIT stage=worktree: changed paths")?;
        paths(&bytes, "RUN_PATHS_UTF8")
    }
    /// Include every delivered commit, not only the final tree diff. A file
    /// added and later deleted is still present in reachable bundle history.
    pub fn commit_evidence(&self) -> Result<Vec<Value>, String> {
        self.commit_evidence_within(EVIDENCE_LIMIT)
    }
    pub fn commit_evidence_within(&self, limit: usize) -> Result<Vec<Value>, String> {
        let mut left = limit;
        let listed = collect(
            self.git().args([
                "rev-list",
                "--reverse",
                &format!("{}..HEAD", self.base_commit),
            ]),
            left,
            "the delivered commit list",
        )?
        .ok_or("RUN_GIT stage=worktree: delivered commits")?;
        left = left.saturating_sub(listed.len());
        let mut result = vec![];
        for commit in String::from_utf8_lossy(&listed).lines() {
            let output = collect(
                self.git().args([
                    "diff-tree",
                    "--root",
                    "-m",
                    "--no-commit-id",
                    "--name-only",
                    "-r",
                    "-z",
                    commit,
                ]),
                left,
                "the delivered commit paths",
            )?
            .ok_or("RUN_GIT stage=worktree: commit paths")?;
            left = left.saturating_sub(output.len());
            let paths = paths(&output, "RUN_COMMIT_PATH_UTF8")?;
            result.push(json!({"sha":commit,"changed_paths":paths}));
        }
        Ok(result)
    }
    /// Complete text blobs clarify evidence that a diff can leave implicit.
    pub fn file_evidence(&self) -> Result<Vec<Value>, String> {
        self.file_evidence_within(EVIDENCE_LIMIT)
    }
    /// All the file contents together stay within `limit`.
    pub fn file_evidence_within(&self, limit: usize) -> Result<Vec<Value>, String> {
        let mut left = limit;
        let mut files = vec![];
        for path in self.changed_paths_within(limit)? {
            let content = self.blob(&path, left)?;
            left = left.saturating_sub(content.as_ref().map_or(0, String::len));
            files.push(json!({"path":path,"complete":content.is_some(),"content":content,"source":"candidate Git commit"}));
        }
        Ok(files)
    }
    fn blob(&self, path: &str, limit: usize) -> Result<Option<String>, String> {
        let bytes = collect(
            self.git().args([
                "show",
                "--no-ext-diff",
                "--no-textconv",
                &format!("HEAD:{path}"),
            ]),
            limit,
            "the candidate file contents",
        )?;
        Ok(bytes.and_then(|b| String::from_utf8(b).ok().filter(|s| !s.contains('\0'))))
    }
}
