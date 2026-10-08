//! Verify every byte while reconstructing CAS blocks with constant storage memory.
use super::*;
use std::fs::File;
use std::io::{self, BufReader, Cursor, Read, Seek, SeekFrom};
struct Reader {
    root: std::path::PathBuf,
    manifest: Segments,
    index: usize,
    block: Cursor<Vec<u8>>,
    hash: Sha256,
    count: usize,
    checked: bool,
}
fn bad() -> io::Error {
    io::Error::other("REVIEW_SOURCE_DIGEST stage=source_stream")
}
impl Read for Reader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.block.read(output)?;
            if n > 0 {
                self.hash.update(&output[..n]);
                self.count += n;
                return Ok(n);
            }
            if let Some(id) = self.manifest.blocks.get(self.index) {
                if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err(bad());
                }
                let bytes = fs::read(self.root.join(format!("block-{id}")))?;
                if format!("{:x}", Sha256::digest(&bytes)) != *id {
                    return Err(bad());
                }
                self.index += 1;
                self.block = Cursor::new(bytes);
                continue;
            }
            if !self.checked {
                if self.count != self.manifest.end
                    || format!("{:x}", self.hash.clone().finalize()) != self.manifest.digest
                {
                    return Err(bad());
                }
                self.checked = true;
            }
            return Ok(0);
        }
    }
}
pub(crate) fn open(path: &Path, digest: &str) -> Result<Box<dyn Read>> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    file.seek(SeekFrom::Start(0))?;
    if format!("{:x}", hash.finalize()) == digest {
        return Ok(Box::new(BufReader::new(file)));
    }
    let manifest: Segments = serde_json::from_reader(BufReader::new(file))
        .map_err(|_| "REVIEW_SOURCE_DIGEST stage=source_stream")?;
    if manifest.kah_source_segments != 1 || manifest.digest != digest {
        return Err("REVIEW_SOURCE_DIGEST stage=source_stream".into());
    }
    Ok(Box::new(Reader {
        root: path.parent().unwrap().into(),
        manifest,
        index: 0,
        block: Cursor::new(vec![]),
        hash: Sha256::new(),
        count: 0,
        checked: false,
    }))
}
