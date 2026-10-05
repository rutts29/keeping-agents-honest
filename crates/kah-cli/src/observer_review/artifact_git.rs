//! Native Git measurements only; no writes, hooks, filters or credentials.
use crate::Result;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub fn git(root: &Path, args: &[&str], input: Option<Vec<u8>>) -> Result<Vec<u8>> {
    let mut command = Command::new("/usr/bin/git");
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .current_dir(root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
            "-c",
        ])
        .arg(format!("safe.directory={}", root.display()))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = if let Some(input) = input {
        let mut child = command.stdin(Stdio::piped()).spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or("REVIEW_ARTIFACT_STDIN stage=artifact_git")?;
        std::thread::scope(|scope| -> Result<_> {
            let writer = scope.spawn(move || stdin.write_all(&input));
            let output = child.wait_with_output()?;
            writer
                .join()
                .map_err(|_| "REVIEW_ARTIFACT_WRITER stage=artifact_git")??;
            Ok(output)
        })?
    } else {
        command.stdin(Stdio::null()).output()?
    };
    if !output.status.success() {
        return Err(format!(
            "REVIEW_ARTIFACT_GIT stage=artifact_git: git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}
pub fn quote(path: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in path.chars() {
        match ch {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(ch)
            }
            c if c.is_ascii_control() => quoted.push_str(&format!("\\{:03o}", c as u32)),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}
