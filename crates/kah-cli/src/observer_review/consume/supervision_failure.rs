//! Supervision belongs to the child request, even if admission advanced meanwhile.
use crate::{Result, observer_review::state::Store};
use serde_json::Value;
use std::{
    error::Error,
    fmt, fs,
    io::{BufRead, BufReader},
};

#[derive(Debug)]
pub struct Failure {
    pub request: Option<String>,
    pub child_pid: u32,
    pub reason: String,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}
impl Error for Failure {}

pub fn reported(
    worker: &str,
    pid: u32,
    started_ms: u64,
    request: &Option<String>,
) -> Result<Option<Value>> {
    let path = Store::directory(worker)?.join("consume-outcomes.jsonl");
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut last = None;
    for line in BufReader::new(file).lines() {
        let line = line?;
        // A concurrent append may end with an incomplete row; no false outcome.
        if let Ok(value) = serde_json::from_str::<Value>(&line)
            && value["pid"] == pid
            && value["at_ms"].as_u64().is_some_and(|at| at >= started_ms)
            && value["event"]["kind"] == "return"
            && value["event"]["success"] == false
            && value["event"]["before"] == serde_json::json!(request)
        {
            last = Some(value["event"].clone());
        }
    }
    Ok(last)
}
