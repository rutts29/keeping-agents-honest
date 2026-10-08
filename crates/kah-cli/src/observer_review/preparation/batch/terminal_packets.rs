//! Terminal packets follow immutable source content across review IDs.
use crate::{
    Result,
    observer_review::{source_store, state::Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path};
pub fn identity(packet: &Value) -> Result<String> {
    let original = super::packet_refs::original(packet)?;
    let prepared = &original["trigger"]["preparation"];
    Ok(kah_observer::context_selection::digest(
        &json!({"turn":original["turn"],
        "purpose":prepared["purpose"],"state":prepared["state"],"questions":prepared["questions"]}),
    ))
}
pub fn record(worker: &str, packet: &Value, receipt: &Value) -> Result<()> {
    let root = Store::directory(worker)?.join("terminal-packets");
    kah_core::run_store::ensure_private_directory(&root)?;
    source_store::save(
        &root,
        &format!("{}.json", identity(packet)?.trim_start_matches("sha256:")),
        &json!({"packet":packet,"receipt":receipt,"terminal":"unconfirmed_context"}),
    )
}
pub fn lookup(worker: &str, packet: &Value) -> Result<Option<Value>> {
    let saved = crate::observer_review::state::read(
        &Store::directory(worker)?
            .join("terminal-packets")
            .join(format!(
                "{}.json",
                identity(packet)?.trim_start_matches("sha256:")
            )),
    )?;
    if saved.is_some() {
        return Ok(saved);
    }
    legacy(worker, packet)
}
fn legacy(worker: &str, packet: &Value) -> Result<Option<Value>> {
    let original = super::packet_refs::original(packet)?;
    let input = &original["trigger"]["preparation"];
    let questions: kah_observer::judge::Questions =
        serde_json::from_value(input["questions"].clone())?;
    let digest = kah_observer::context_selection::digest(
        &json!({"original":input["state"],"questions":questions,"packet":packet}),
    );
    let root = Store::directory(worker)?;
    for entry in fs::read_dir(&root)? {
        let path = entry?.path();
        if !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("packet-result-"))
        {
            continue;
        }
        use crate::observer_review::source_store::{Select, read_selected};
        let metadata = read_selected(
            &path,
            &Select::fields(&[("terminal", Select::All), ("original_digest", Select::All)]),
        )?;
        if metadata["terminal"] == "unconfirmed_context" && metadata["original_digest"] == digest {
            let receipt: Value = crate::observer_review::state::read(&path)?
                .ok_or("PREPARATION_PACKET_RESULT stage=terminal")?;
            record(worker, packet, &receipt)?;
            return Ok(Some(
                json!({"packet":packet,"receipt":receipt,"terminal":"unconfirmed_context"}),
            ));
        }
    }
    Ok(None)
}
pub fn retire(
    worker: &str,
    header: &crate::observer_review::queue::Wake,
    path: &Path,
) -> Result<bool> {
    let (wake, _) = super::packet_refs::group(worker, header, &[path.to_owned()])?;
    let packet = &wake.trigger["preparation_packets"][0];
    let Some(terminal) = lookup(worker, packet)? else {
        return Ok(false);
    };
    let root = Store::directory(worker)?;
    let retired = root.join("covered-triggers");
    let links = root.join("terminal-packet-links");
    kah_core::run_store::ensure_private_directory(&retired)?;
    kah_core::run_store::ensure_private_directory(&links)?;
    let name = path
        .file_name()
        .ok_or("PREPARATION_PACKET_PATH stage=queue")?;
    source_store::save(
        &links,
        name.to_str().ok_or("PREPARATION_PACKET_PATH stage=queue")?,
        &json!({"original":name,"covered_by":terminal["receipt"]["decision"],"packet_identity":identity(packet)?,
            "terminal":"unconfirmed_context","original_retained":retired.join(name)}),
    )?;
    if retired.join(name).exists() {
        if fs::read(path)? != fs::read(retired.join(name))? {
            return Err("PREPARATION_PACKET_CHANGED stage=queue".into());
        }
        fs::remove_file(path)?;
    } else {
        fs::rename(path, retired.join(name))?;
    }
    Ok(true)
}
