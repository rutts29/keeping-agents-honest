//! Stage telemetry, not a limit or a reason to terminate progressing work.
use super::*;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
pub fn sample(worker: &str, stage: &str) -> Result<()> {
    let status = fs::read_to_string("/proc/self/status")?;
    let mut memory = serde_json::Map::new();
    for (name, line) in [
        ("rss_kib", "VmRSS:"),
        ("peak_rss_kib", "VmHWM:"),
        ("swap_kib", "VmSwap:"),
    ] {
        let value = status
            .lines()
            .find(|s| s.starts_with(line))
            .and_then(|s| s.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u64>().ok());
        memory.insert(name.into(), json!(value));
    }
    let root = Store::directory(worker)?;
    kah_core::run_store::ensure_private_directory(&root)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("admission-memory.jsonl"))?;
    writeln!(
        file,
        "{}",
        json!({"stage":stage,"pid":std::process::id(),"at_ms":kah_observer::session::now_ms(),"memory":memory})
    )?;
    Ok(())
}
