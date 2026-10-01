//! Native command transport through a private, launch-owned executor specification.
use crate::Result;
use kah_core::run_store::ensure_private_directory;
use kah_sandbox::{Executor, current_uid};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub executor: Executor,
    pub temporary: Option<PathBuf>,
    pub service_uid: u32,
}
impl Spec {
    pub fn save(&self, path: &Path) -> Result<()> {
        self.write(path)
            .map_err(|e| format!("SANDBOX_SPEC_WRITE stage=prepare: {e}").into())
    }
    fn write(&self, path: &Path) -> Result<()> {
        let parent = path.parent().ok_or("SANDBOX_SPEC_PATH stage=prepare")?;
        ensure_private_directory(parent)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        let result = (|| -> Result<()> {
            serde_json::to_writer(&mut file, self)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn load(path: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path).map_err(read_error)?;
        if !metadata.is_file() || metadata.uid() != current_uid()? || metadata.mode() & 0o022 != 0 {
            return Err("SANDBOX_SPEC_OWNER stage=command: expected an operator-owned private specification".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(read_error)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(read_error)?;
        if bytes.len() > 1024 * 1024 {
            return Err("SANDBOX_SPEC_BOUND stage=command".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "SANDBOX_SPEC_JSON stage=command: invalid specification".into())
    }
    pub fn command(&self, cwd: &Path, command: &[String]) -> Result<Command> {
        self.prepare_temporary()?;
        Ok(self.executor.command(cwd, command)?)
    }
    fn prepare_temporary(&self) -> Result<()> {
        let Some(path) = &self.temporary else {
            return Ok(());
        };
        let metadata = fs::symlink_metadata(path)
            .map_err(|e| format!("SANDBOX_TEMP_READ stage=command: {e}"))?;
        if !metadata.is_dir() || metadata.uid() != current_uid()? {
            return Err("SANDBOX_TEMP_TYPE stage=command".into());
        }
        // Native Claude creates private cwd/temp files between invocations.
        // execdir avoids following replaceable intermediate path components;
        // physical traversal and setfacl -P skip links, and no ownership changes.
        let output = Command::new("/usr/bin/find")
            .arg("-P")
            .arg(path)
            .args([
                "-uid",
                &current_uid()?.to_string(),
                "-execdir",
                "/usr/bin/setfacl",
                "-P",
                "-m",
                &format!("u:{}:rwX", self.service_uid),
                "--",
                "{}",
                "+",
            ])
            .env("PATH", "/usr/bin:/bin")
            .output()
            .map_err(|e| format!("SANDBOX_TEMP_ACCESS stage=command: {e}"))?;
        if !output.status.success() {
            return Err(
                "SANDBOX_TEMP_ACCESS stage=command: cannot share native temporary files".into(),
            );
        }
        Ok(())
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let command = match args.first().map(String::as_str) {
        Some("--sandbox-shell") if args.len() == 3 => {
            vec!["/bin/bash".into(), "-c".into(), args[2].clone()]
        }
        Some("--sandbox-exec") if args.len() > 2 => args[2..].to_vec(),
        _ => return Err("SANDBOX_COMMAND_ARGUMENTS stage=command".into()),
    };
    let path = args.get(1).ok_or("SANDBOX_SPEC_REQUIRED stage=command")?;
    let spec = Spec::load(Path::new(path))?;
    let cwd = std::env::current_dir().map_err(|e| format!("SANDBOX_CWD stage=command: {e}"))?;
    Err(format!(
        "SANDBOX_COMMAND_EXEC stage=command: {}",
        spec.command(&cwd, &command)?.exec()
    )
    .into())
}

fn read_error(error: std::io::Error) -> String {
    format!("SANDBOX_SPEC_READ stage=command: {error}")
}
