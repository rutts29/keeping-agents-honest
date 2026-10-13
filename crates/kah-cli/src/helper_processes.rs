//! Install provenance only: no signals, state writes or native provider calls.
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, os::unix::fs::MetadataExt, path::Path};

pub fn collect() -> Result<Vec<Value>> {
    let mut records = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let root = entry.path();
        let Ok(args) = fs::read(root.join("cmdline")) else {
            continue;
        };
        let Some(command) = args.split(|b| *b == 0).nth(1) else {
            continue;
        };
        let Ok(command) = std::str::from_utf8(command) else {
            continue;
        };
        if !crate::helper_upgrade::long_lived(std::ffi::OsStr::new(command)) {
            continue;
        }
        let record = snapshot(pid, command, &root).unwrap_or_else(|e| {
            json!({"pid":pid,"command":command,"exe_sha256":null,"error":format!("HELPER_INVENTORY stage=install: {e}")})
        });
        records.push(record);
    }
    records.sort_by_key(|v| v["pid"].as_u64());
    Ok(records)
}

fn snapshot(pid: u32, command: &str, root: &Path) -> Result<Value> {
    let before = fs::read_to_string(root.join("stat"))?;
    let mut file = fs::File::open(root.join("exe"))?;
    let opened = file.metadata()?;
    let executable = fs::read_link(root.join("exe"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    let after = fs::read_to_string(root.join("stat"))?;
    let current = fs::metadata(root.join("exe"))?;
    let stamp = |text: &str| {
        text.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().nth(19))
            .map(str::to_owned)
    };
    if stamp(&before).is_none()
        || stamp(&before) != stamp(&after)
        || opened.dev() != current.dev()
        || opened.ino() != current.ino()
    {
        return Err("HELPER_PID_CHANGED stage=install".into());
    }
    Ok(
        json!({"pid":pid,"command":command,"process_stamp":stamp(&after),
        "exe":executable,"exe_sha256":format!("{:x}",hash.finalize())}),
    )
}

#[cfg(test)]
#[path = "../tests/observer_unit/helper_processes.rs"]
mod tests;
