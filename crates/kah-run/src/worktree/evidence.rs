use super::{Worktree, io_error};
use serde_json::{Value, json};
use std::{io::Read, process::Stdio};
impl Worktree {
    /// Include every delivered commit, not only the final tree diff. A file
    /// added and later deleted is still present in reachable bundle history.
    pub fn commit_evidence(&self) -> Result<Vec<Value>, String> {
        let commits = super::text(super::run(self.git().args([
            "rev-list",
            "--reverse",
            "--max-count=65",
            &format!("{}..HEAD", self.base_commit),
        ]))?)?;
        if commits.lines().count() > 64 {
            return Err("RUN_COMMITS_BOUND stage=completion".into());
        }
        let mut result = vec![];
        for commit in commits.lines() {
            let output = super::run(self.git().args([
                "diff-tree",
                "--root",
                "-m",
                "--no-commit-id",
                "--name-only",
                "-r",
                "-z",
                commit,
            ]))?;
            if output.stdout.len() > 65536 {
                return Err("RUN_COMMIT_PATHS_BOUND stage=completion".into());
            }
            let paths = output
                .stdout
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .map(|p| {
                    String::from_utf8(p.to_vec())
                        .map_err(|_| "RUN_COMMIT_PATH_UTF8 stage=completion")
                })
                .collect::<Result<Vec<_>, _>>()?;
            result.push(json!({"sha":commit,"changed_paths":paths}));
        }
        Ok(result)
    }
    /// Bounded complete blobs clarify language/library evidence that a diff can
    /// leave implicit. Omitted data is marked explicitly, never called complete.
    pub fn file_evidence(&self) -> Result<Vec<Value>, String> {
        let mut budget = 16 * 1024usize;
        let mut files = vec![];
        for path in self.changed_paths()?.into_iter().take(8) {
            let content = self.blob(&path, budget.min(4096))?;
            if let Some(text) = &content {
                budget = budget.saturating_sub(text.len());
            }
            files.push(json!({"path":path,"complete":content.is_some(),"content":content,"source":"candidate Git commit"}));
        }
        Ok(files)
    }
    fn blob(&self, path: &str, limit: usize) -> Result<Option<String>, String> {
        let mut child = self
            .git()
            .args([
                "show",
                "--no-ext-diff",
                "--no-textconv",
                &format!("HEAD:{path}"),
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
            return Ok(None);
        }
        if !child.wait().map_err(io_error)?.success() {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes).ok().filter(|s| !s.contains('\0')))
    }
}
