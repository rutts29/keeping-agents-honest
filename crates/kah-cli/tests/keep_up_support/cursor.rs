//! Incremental metadata cursor; immutable sources are decoded once, not capped.
use super::*;
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
};
#[derive(Default)]
pub struct Cursor {
    pub decoded: u64,
    pub(super) commits: BTreeMap<String, u64>,
    metadata: BTreeMap<PathBuf, (Stamp, Value)>,
    event_inode: Option<u64>,
    event_offset: u64,
    event_tail: Vec<u8>,
    events: Vec<Value>,
}
type Stamp = (u64, u64, i64, i64, i64, i64);
impl Cursor {
    pub(super) fn events(&mut self, path: &Path, gaps: &mut Vec<Value>) -> Vec<Value> {
        let result = (|| -> Result<()> {
            let mut file = fs::File::open(path)?;
            let meta = file.metadata()?;
            if self.event_inode != Some(meta.ino()) || meta.len() < self.event_offset {
                self.event_inode = Some(meta.ino());
                self.event_offset = 0;
                self.event_tail.clear();
                self.events.clear();
            }
            file.seek(SeekFrom::Start(self.event_offset))?;
            let size = file.read_to_end(&mut self.event_tail)?;
            self.event_offset += size as u64;
            if let Some(end) = self.event_tail.iter().rposition(|b| *b == b'\n') {
                let complete = self.event_tail.drain(..=end).collect::<Vec<_>>();
                for line in complete.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
                    match serde_json::from_slice::<Value>(line) {
                        Ok(v) => self.events.push(v),
                        Err(e) => gaps.push(json!({"path":path,"error":e.to_string()})),
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result
            && path.exists()
        {
            gaps.push(json!({"path":path,"error":e.to_string()}));
        }
        self.events.clone()
    }
    pub(super) fn select(&mut self, path: &Path, selection: &Select) -> Result<Value> {
        let meta = fs::metadata(path)?;
        let stamp = (
            meta.ino(),
            meta.len(),
            meta.mtime(),
            meta.mtime_nsec(),
            meta.ctime(),
            meta.ctime_nsec(),
        );
        if let Some((old, value)) = self.metadata.get(path)
            && *old == stamp
        {
            return Ok(value.clone());
        }
        let value = read_selected(path, selection)?;
        self.decoded += 1;
        self.metadata
            .insert(path.to_owned(), (stamp, value.clone()));
        Ok(value)
    }
    pub(super) fn optional(
        &mut self,
        path: &Path,
        fields: &[&str],
        gaps: &mut Vec<Value>,
    ) -> Value {
        if !path.exists() {
            return Value::Null;
        }
        let fields = fields.iter().map(|n| (*n, Select::All)).collect::<Vec<_>>();
        match self.select(path, &Select::fields(&fields)) {
            Ok(v) => v,
            Err(e) => {
                gaps.push(json!({"path":path,"error":e.to_string()}));
                Value::Null
            }
        }
    }
}
