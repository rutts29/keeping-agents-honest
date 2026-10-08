//! Admission reads only plain control fields; complete evidence stays archived.
use super::*;
use serde::Deserialize;
use std::io::BufReader;
#[derive(Deserialize)]
pub struct Control {
    pub worker: Endpoint,
    pub observer: Observer,
    pub pending: Option<Pending>,
    pub stopped: bool,
}
pub fn read_control(worker: &str) -> Result<Option<Control>> {
    let path = Store::directory(worker)?.join("state.json");
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("REVIEW_STATE_PRIVATE stage=control".into());
    }
    // Serde's ignored-field reader streams past latest/deferrals without
    // resolving their CAS payloads. No text is cut or changed for decisions.
    Ok(Some(
        serde_json::from_reader(BufReader::new(file))
            .map_err(|e| format!("REVIEW_STATE_JSON stage=control: {e}"))?,
    ))
}
