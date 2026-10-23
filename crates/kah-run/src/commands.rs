//! Operator requests use the private state lock, never the worker's output.
use crate::store::{Record, Store};
use std::{fs, io::Read, path::Path};

pub fn steer(base: &Path, id: &str, text: &str) -> Result<u64, String> {
    let mut store = Store::open_wait(base, id)?;
    let mut record = store.load()?;
    let sequence = record.control.steer(text)?;
    if let Some(native) = &mut record.native {
        native.pending.push_back(sequence);
    }
    store.save(&record)?;
    Ok(sequence)
}
pub fn stop(base: &Path, id: &str) -> Result<(), String> {
    let mut store = Store::open_wait(base, id)?;
    let mut record = store.load()?;
    record.control.request_stop();
    store.save(&record)
}
pub fn view(base: &Path, id: &str) -> Result<Record, String> {
    Store::read(base, id)
}
/// Every readable run, sorted by id. A run that cannot be read is reported on
/// stderr and skipped, so one half-created or damaged run does not hide the rest.
pub fn list(base: &Path) -> Result<Vec<(String, Record)>, String> {
    let (runs, problems) = list_with_problems(base)?;
    for (id, reason) in problems {
        eprintln!("kah: RUN_LIST_SKIPPED stage=run_list: {id}: {reason}");
    }
    Ok(runs)
}
/// Like [`list`], with the runs that could not be read and why, sorted by id.
/// Failing to read the base directory itself is still an error.
pub fn list_with_problems(base: &Path) -> Result<(Vec<(String, Record)>, Problems), String> {
    let entries = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((vec![], vec![])),
        Err(e) => return Err(io_error(e)),
    };
    let (mut runs, mut problems) = (vec![], vec![]);
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if id.starts_with("run-") {
            match view(base, &id) {
                Ok(record) => runs.push((id, record)),
                Err(reason) => problems.push((id, reason)),
            }
        }
    }
    runs.sort_by(|a, b| a.0.cmp(&b.0));
    problems.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((runs, problems))
}
/// Run ids that were skipped, each with the reason.
pub type Problems = Vec<(String, String)>;
pub fn logs(base: &Path, id: &str) -> Result<String, String> {
    let directory = Store::path(base, id)?;
    let mut files = fs::read_dir(directory)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?;
    files.sort_by_key(|entry| entry.file_name());
    let mut output = String::new();
    for entry in files {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("attempt-") || !name.ends_with(".log") {
            continue;
        }
        if !entry.file_type().map_err(io_error)?.is_file() {
            return Err("RUN_LOG_TYPE stage=run_logs: only regular logs are read".into());
        }
        let remaining = (8 * 1024 * 1024usize).saturating_sub(output.len());
        let mut bytes = Vec::new();
        fs::File::open(entry.path())
            .map_err(io_error)?
            .take(remaining as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > remaining {
            return Err("RUN_LOG_BOUND stage=run_logs: inspect the private log files directly for larger output".into());
        }
        output.push_str(&String::from_utf8_lossy(&bytes));
    }
    Ok(output)
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_CONTROL_IO stage=run_commands: {error}")
}
