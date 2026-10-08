//! Host progress means new decision stages or changed result content, not rewrites.
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::BufRead, path::Path};

pub(super) fn combined(
    root: &Path,
    outbox: &Path,
    request: &str,
    submitted: &Path,
) -> Result<Option<(u64, u64)>> {
    Ok([
        super::monitor::report_version(outbox, request, submitted),
        version(root, request)?,
    ]
    .into_iter()
    .flatten()
    .reduce(|(size, time), (next, at)| (size.saturating_add(next), time.max(at))))
}
pub(super) fn version(root: &Path, request: &str) -> Result<Option<(u64, u64)>> {
    let stages = stages(root, request)?;
    let directory = root.join("liveness/host-progress");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let name = format!("{request}.json");
    let prior: Option<Value> = crate::observer_review::state::read(&directory.join(&name))?;
    let results = results(root, request, prior.as_ref())?;
    let values = stages.values().map(|at| (1, *at)).chain(
        results
            .values()
            .map(|v| (v["bytes"].as_u64().unwrap(), v["at_ms"].as_u64().unwrap())),
    );
    let version =
        values.reduce(|(size, time), (next, at)| (size.saturating_add(next), time.max(at)));
    let state = json!({"stages":stages,"results":results});
    if prior.as_ref() != Some(&state) {
        crate::unattended_report::save(&directory, &name, &state)?;
    }
    Ok(version)
}
fn stages(root: &Path, request: &str) -> Result<BTreeMap<String, u64>> {
    let mut stages = BTreeMap::new();
    let file = match fs::File::open(root.join("consume-outcomes.jsonl")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(stages),
        Err(e) => return Err(e.into()),
    };
    for line in std::io::BufReader::new(file).lines() {
        let Ok(row) = serde_json::from_str::<Value>(&line?) else {
            continue;
        };
        if row["event"]["request"] == request
            && row["event"]["kind"] == "stage"
            && let (Some(stage), Some(at)) = (row["event"]["stage"].as_str(), row["at_ms"].as_u64())
        {
            stages.entry(stage.to_owned()).or_insert(at);
        }
    }
    Ok(stages)
}
fn results(root: &Path, request: &str, prior: Option<&Value>) -> Result<BTreeMap<String, Value>> {
    let mut results = BTreeMap::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&format!("packet-result-{request}-"))
            && !name.starts_with(&format!("prepared-{request}-"))
        {
            continue;
        }
        let Some((bytes, at)) = super::monitor::version(&path) else {
            continue;
        };
        let digest = digest(&path)?;
        let previous = prior.and_then(|v| v["results"].get(name));
        let at = previous
            .filter(|v| v["digest"] == digest)
            .and_then(|v| v["at_ms"].as_u64())
            .unwrap_or(at);
        results.insert(
            name.to_owned(),
            json!({"digest":digest,"bytes":bytes,"at_ms":at}),
        );
    }
    Ok(results)
}
fn digest(path: &Path) -> Result<String> {
    let mut reader = std::io::BufReader::new(fs::File::open(path)?);
    let mut hash = Sha256::new();
    loop {
        let size = {
            let bytes = reader.fill_buf()?;
            if bytes.is_empty() {
                break;
            }
            hash.update(bytes);
            bytes.len()
        };
        reader.consume(size);
    }
    Ok(format!("{:x}", hash.finalize()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_cycles_and_identical_result_rewrites_do_not_manufacture_progress() {
        let root =
            std::env::temp_dir().join(format!("kah-host-progress-{}", crate::relay_store::nonce()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("consume-outcomes.jsonl");
        let stage = |at, request| {
            json!({"at_ms":at,"event":{"request":request,"kind":"stage","stage":"decision_started:a"}}).to_string()
        };
        fs::write(&path, format!("{}\n", stage(10, "review-a"))).unwrap();
        let first = version(&root, "review-a").unwrap();
        fs::write(
            &path,
            format!(
                "{}\n{}\n{}\n",
                stage(10, "review-a"),
                stage(20, "review-a"),
                stage(30, "review-other")
            ),
        )
        .unwrap();
        assert_eq!(version(&root, "review-a").unwrap(), first);
        let result = root.join("packet-result-review-a-packet-one.json");
        fs::write(&result, "new host result").unwrap();
        let next = version(&root, "review-a").unwrap();
        assert_ne!(next, first);
        fs::write(&result, "new host result").unwrap();
        assert_eq!(version(&root, "review-a").unwrap(), next);
        fs::write(&result, "changed host decision").unwrap();
        assert_ne!(version(&root, "review-a").unwrap(), next);
        assert_eq!(version(&root, "missing-review").unwrap(), None);
        fs::remove_dir_all(root).unwrap();
    }
}
