//! Storage blocks only: every byte is reconstructed before use, never selected.
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize)]
struct Segments {
    kah_source_segments: u8,
    end: usize,
    digest: String,
    blocks: Vec<String>,
}
// Storage granularity, not an input/content budget. UTF-8 is decoded only after
// joining every block, so a boundary cannot cut the text seen by an observer.
const BLOCK: usize = 64 * 1024;

pub(super) fn store(root: &Path, bytes: &[u8], digest: &str) -> Result<()> {
    if bytes.len() <= BLOCK {
        return super::write_once(&root.join(digest), bytes);
    }
    let mut blocks = Vec::new();
    for block in bytes.chunks(BLOCK) {
        let id = format!("{:x}", Sha256::digest(block));
        super::write_once(&root.join(format!("block-{id}")), block)?;
        blocks.push(id);
    }
    let manifest = Segments {
        kah_source_segments: 1,
        end: bytes.len(),
        digest: digest.into(),
        blocks,
    };
    super::write_once(&root.join(digest), &serde_json::to_vec(&manifest)?)
}

pub(super) fn recover(path: &Path, digest: &str) -> Result<Vec<u8>> {
    let stored = super::read_bytes(path)?;
    if format!("{:x}", Sha256::digest(&stored)) == digest {
        return Ok(stored); // Older, unsegmented sources remain readable.
    }
    let manifest: Segments = serde_json::from_slice(&stored)
        .map_err(|_| "REVIEW_SOURCE_DIGEST stage=source_segments")?;
    if manifest.kah_source_segments != 1 || manifest.digest != digest {
        return Err("REVIEW_SOURCE_DIGEST stage=source_segments".into());
    }
    let mut bytes = Vec::new();
    for id in manifest.blocks {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("REVIEW_SOURCE_ID stage=source_segments".into());
        }
        let block_path = path.parent().unwrap().join(format!("block-{id}"));
        let block = super::read_bytes(&block_path)?;
        if format!("{:x}", Sha256::digest(&block)) != id {
            return Err("REVIEW_SOURCE_DIGEST stage=source_segments".into());
        }
        bytes.extend(block);
    }
    if bytes.len() != manifest.end || format!("{:x}", Sha256::digest(&bytes)) != digest {
        return Err("REVIEW_SOURCE_DIGEST stage=source_segments".into());
    }
    Ok(bytes)
}

pub(super) fn migrate(root: &Path) -> Result<usize> {
    let mut migrated = 0;
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let name = path.file_name().unwrap().to_string_lossy();
        if name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let bytes = recover(&path, &name)?;
        if format!("{:x}", Sha256::digest(fs::read(&path)?)) != name {
            continue;
        }
        if bytes.len() <= BLOCK {
            continue;
        }
        let staging = root.join(format!("segments-{}.tmp", crate::relay_store::nonce()));
        fs::create_dir(&staging)?;
        store(&staging, &bytes, &name)?;
        if recover(&staging.join(name.as_ref()), &name)? != bytes {
            return Err("REVIEW_SOURCE_ROUNDTRIP stage=source_segments".into());
        }
        for block in fs::read_dir(&staging)? {
            let block = block?;
            if block.file_name() == path.file_name().unwrap() {
                continue;
            }
            super::write_once(&root.join(block.file_name()), &fs::read(block.path())?)?;
        }
        let replacement = root.join(format!("replacement-{}.tmp", crate::relay_store::nonce()));
        super::write_once(&replacement, &fs::read(staging.join(name.as_ref()))?)?;
        // Reused destination blocks also need verification. Checking staging
        // alone would miss a corrupt block already present in the shared store.
        if recover(&replacement, &name)? != bytes {
            return Err("REVIEW_SOURCE_ROUNDTRIP stage=source_segments".into());
        }
        fs::rename(replacement, &path)?;
        fs::File::open(root)?.sync_all()?;
        fs::remove_dir_all(staging)?;
        migrated += 1;
    }
    Ok(migrated)
}

pub(super) mod reader;
