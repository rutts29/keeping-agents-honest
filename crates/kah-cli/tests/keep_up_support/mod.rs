use kah_cli::observer_review::source_store::{Select, read_selected};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
mod cursor;
mod recording;
pub use recording::{Recording, reconstruct};
mod snapshot;
pub use cursor::Cursor;
mod stats;
pub use stats::summary;
pub fn sample(root: &Path, repo: &Path) -> Value {
    snapshot::read(root, repo)
}
pub fn sample_cursor(root: &Path, repo: &Path, cursor: &mut Cursor) -> Value {
    snapshot::read_cached(root, repo, cursor)
}

pub fn run() -> Result<()> {
    let root = fs::canonicalize(std::env::var("KAH_KEEP_UP_REVIEW_ROOT")?)?;
    let repo = fs::canonicalize(std::env::var("KAH_KEEP_UP_REPO")?)?;
    let output = PathBuf::from(std::env::var("KAH_KEEP_UP_OUTPUT")?);
    if !output.starts_with("/mnt/kah-fixtures/") || output.exists() {
        return Err("KEEP_UP_OUTPUT stage=record: require new private volume directory".into());
    }
    let parent = fs::canonicalize(output.parent().ok_or("KEEP_UP_OUTPUT stage=record")?)?;
    if !parent.starts_with("/mnt/kah-fixtures/")
        || parent.starts_with(&root)
        || root.starts_with(&parent)
        || parent.starts_with(&repo)
    {
        return Err("KEEP_UP_OUTPUT stage=record: parent overlaps source or leaves volume".into());
    }
    kah_core::run_store::ensure_private_directory(&output)?;
    let output = fs::canonicalize(output)?;
    if output.starts_with(&root) || root.starts_with(&output) || output.starts_with(&repo) {
        return Err("KEEP_UP_OUTPUT stage=record: output overlaps source".into());
    }
    record(
        &root,
        &repo,
        &output,
        std::env::var_os("KAH_KEEP_UP_ONCE").is_some(),
    )
}
fn record(root: &Path, repo: &Path, output: &Path, once: bool) -> Result<()> {
    let mut log = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("samples.jsonl"))?;
    let mut samples = vec![];
    let mut recording = Recording::default();
    let mut cursor = Cursor::default();
    let mut event_log = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("liveness-events.jsonl"))?;
    let mut archived = 0;
    loop {
        disk_guard()?;
        let mut sample = snapshot::read_cached(root, repo, &mut cursor);
        archive_events(&mut sample, &mut event_log, &mut archived)?;
        let delta = recording.encode(output, &sample)?;
        writeln!(log, "{delta}")?;
        log.sync_all()?;
        samples.push(recording::metrics_sample(&sample));
        let mut measured = summary(&samples);
        measured["recorder_bytes"] =
            json!({"samples":log.metadata()?.len(),"liveness_events":event_log.metadata()?.len()});
        fs::write(
            output.join("summary.json"),
            serde_json::to_vec_pretty(&measured)?,
        )?;
        if once || output.join("STOP").exists() {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    Ok(())
}
fn disk_guard() -> Result<()> {
    let result = std::process::Command::new("df")
        .args(["-B1", "--output=avail", "/", "/mnt/kah-fixtures"])
        .output()?;
    if !result.status.success() {
        return Err("KEEP_UP_DISK stage=record: df failed".into());
    }
    for available in String::from_utf8(result.stdout)?.lines().skip(1) {
        if available.trim().parse::<u64>()? < 4 * 1024 * 1024 * 1024 {
            return Err(
                "KEEP_UP_DISK stage=record: stop below 4 GiB; retained output, no cleanup".into(),
            );
        }
    }
    Ok(())
}
fn archive_events(sample: &mut Value, output: &mut fs::File, archived: &mut usize) -> Result<()> {
    let events = sample["counters"]
        .as_object_mut()
        .and_then(|o| o.remove("liveness_events"))
        .unwrap_or(json!([]));
    let events = events.as_array().ok_or("KEEP_UP_EVENTS stage=record")?;
    if events.len() < *archived {
        writeln!(
            output,
            "{}",
            json!({"source_reset":true,"at_ms":sample["at_ms"]})
        )?;
        *archived = 0;
    }
    for event in &events[*archived..] {
        writeln!(output, "{event}")?;
    }
    *archived = events.len();
    output.sync_all()?;
    Ok(())
}
fn paths(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    Ok(paths)
}
fn modified(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}
