//! A batch retains immutable packet files, not cloned whole states in a vector.
use super::*;
use crate::observer_review::{queue::Wake, state::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub fn group(worker: &str, header: &Wake, paths: &[PathBuf]) -> Result<(Wake, Vec<PathBuf>)> {
    let root = Store::directory(worker)?;
    let archive = root.join("preparation-packets");
    kah_core::run_store::ensure_private_directory(&archive)?;
    let mut packets = vec![];
    for path in paths {
        let bytes = crate::observer_review::consume::read_report(path)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let file = archive.join(format!("{digest}.json"));
        if file.exists() {
            if fs::read(&file)? != bytes {
                return Err("PREPARATION_PACKET_CHANGED stage=queue".into());
            }
        } else {
            fs::copy(path, &file)?;
        }
        let raw: Wake = serde_json::from_slice(&bytes)?;
        packets.push(json!({"source":path.file_name().unwrap(),"turn":raw.turn,"stored_packet":{"path":file,"digest":digest}}));
    }
    let mut wake = header.clone();
    wake.trigger["preparation_packets"] = json!(packets);
    Ok((wake, paths.to_vec()))
}
pub fn original(packet: &Value) -> Result<Value> {
    let Some(reference) = packet.get("stored_packet") else {
        return Ok(packet.clone());
    };
    let path = Path::new(
        reference["path"]
            .as_str()
            .ok_or("PREPARATION_PACKET_REFERENCE stage=context_preparation")?,
    );
    let bytes = crate::observer_review::consume::read_report(path)?;
    if reference["digest"] != format!("{:x}", Sha256::digest(&bytes)) {
        return Err("PREPARATION_PACKET_CHANGED stage=context_preparation".into());
    }
    let wake: Wake = crate::observer_review::source_store::read(path)?;
    if packet["turn"] != wake.turn {
        return Err("PREPARATION_PACKET_CHANGED stage=context_preparation".into());
    }
    Ok(json!({"source":packet["source"],"turn":wake.turn,"trigger":wake.trigger}))
}
