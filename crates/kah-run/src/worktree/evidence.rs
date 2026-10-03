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
            &format!("{}..HEAD", self.base_commit),
        ]))?)?;
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
    /// Complete text blobs clarify evidence that a diff can leave implicit.
    pub fn file_evidence(&self) -> Result<Vec<Value>, String> {
        let mut files = vec![];
        for path in self.changed_paths()? {
            let content = self.blob(&path)?;
            files.push(json!({"path":path,"complete":content.is_some(),"content":content,"source":"candidate Git commit"}));
        }
        Ok(files)
    }
    fn blob(&self, path: &str) -> Result<Option<String>, String> {
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
        let read = child.stdout.take().unwrap().read_to_end(&mut bytes);
        if read.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("RUN_BLOB_READ stage=completion".into());
        }
        if !child.wait().map_err(io_error)?.success() {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes).ok().filter(|s| !s.contains('\0')))
    }
}
