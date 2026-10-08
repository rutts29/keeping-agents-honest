//! Storage-only profiling of retained, scrubbed evidence; no native agent turn.
use kah_cli::observer_review::source_store;
use serde_json::{Value, json};
use std::{fs, io::BufReader, path::PathBuf};

fn sample(stage: &str) -> Result<(), Box<dyn std::error::Error>> {
    let status = fs::read_to_string("/proc/self/status")?;
    let memory = ["VmRSS:", "VmHWM:", "VmSwap:"]
        .iter()
        .map(|key| {
            status
                .lines()
                .find(|line| line.starts_with(key))
                .unwrap_or("")
        })
        .collect::<Vec<_>>();
    println!("{}", json!({"stage":stage,"memory":memory}));
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: source_publication_memory INPUT FRESH_PRIVATE_DIRECTORY".into());
    }
    let root = PathBuf::from(&args[1]);
    if root.exists() {
        return Err("profile directory must not exist".into());
    }
    kah_core::run_store::ensure_private_directory(&root)?;
    source_store::initialize(&root, &root.join("workspace"))?;
    sample("entered")?;
    let value: Value = serde_json::from_reader(BufReader::new(fs::File::open(&args[0])?))?;
    sample("original_loaded")?;
    source_store::save(&root, "request.json", &value)?;
    sample("source_published_and_verified")?;
    println!(
        "{}",
        json!({"input_bytes":fs::metadata(&args[0])?.len(),"record_bytes":fs::metadata(root.join("request.json"))?.len()})
    );
    Ok(())
}
