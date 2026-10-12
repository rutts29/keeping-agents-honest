//! Read only appended complete native records; no size/time/content ceiling.
use super::*;
use crate::observer_review::{source_store, state};
use std::{fs, os::unix::fs::MetadataExt};
#[derive(Serialize, Deserialize)]
struct Cursor {
    observer: Option<String>,
    source: PathBuf,
    device: u64,
    inode: u64,
    offset: u64,
}
pub(super) fn observe(state: &Control) -> Result<Option<NativeBlock>> {
    let Some(path) = state.observer.transcript.as_deref() else {
        return Ok(None);
    };
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!("REVIEW_PROVIDER_SOURCE_IO stage=native_provider: {e}").into());
        }
    };
    let root = Store::directory(&state.worker.session)?.join("provider-block");
    let _lock = Store::delivery_at(&root, true)?;
    let old: Option<Cursor> = state::read(&root.join("cursor.json"))?;
    let start = old
        .filter(|c| {
            c.observer == state.observer.session
                && c.source == path
                && c.device == meta.dev()
                && c.inode == meta.ino()
                && c.offset <= meta.len()
        })
        .map_or(0, |c| c.offset);
    if start == meta.len() {
        return Ok(None);
    }
    let (found, offset) = super::reader::scan(&state.worker.provider, path, start)?;
    if found.is_none() && offset != start {
        let cursor = Cursor {
            observer: state.observer.session.clone(),
            source: path.to_owned(),
            device: meta.dev(),
            inode: meta.ino(),
            offset,
        };
        source_store::save(&root, "cursor.json", &serde_json::to_value(cursor)?)?;
    }
    Ok(found)
}
