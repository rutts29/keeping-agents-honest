//! Meaningful host stages/results count as progress, not idle native activity.
use crate::Result;
use serde_json::Value;
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
    let mut stages = BTreeMap::new();
    match fs::File::open(root.join("consume-outcomes.jsonl")) {
        Ok(file) => {
            for line in std::io::BufReader::new(file).lines() {
                let line = line?;
                let Ok(row) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if row["event"]["request"] == request
                    && row["event"]["kind"] == "stage"
                    && let (Some(stage), Some(at)) =
                        (row["event"]["stage"].as_str(), row["at_ms"].as_u64())
                {
                    // Repeated start/return cycles do not manufacture progress.
                    stages.entry(stage.to_owned()).or_insert(at);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let mut versions = stages.values().map(|at| (1, *at)).collect::<Vec<_>>();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with(&format!("packet-result-{request}-"))
                    || name.starts_with(&format!("prepared-{request}-"))
            })
            && let Some(version) = super::monitor::version(&path)
        {
            versions.push(version);
        }
    }
    Ok(versions
        .into_iter()
        .reduce(|(size, time), (next, at)| (size.saturating_add(next), time.max(at))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_new_current_request_stages_and_result_writes_count() {
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
        fs::write(
            root.join("packet-result-review-a-packet-one.json"),
            "new host result",
        )
        .unwrap();
        let next = version(&root, "review-a").unwrap();
        assert_ne!(next, first);
        assert_eq!(version(&root, "missing-review").unwrap(), None);
        fs::remove_dir_all(root).unwrap();
    }
}
