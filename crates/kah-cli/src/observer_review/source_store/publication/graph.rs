use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::MetadataExt;
fn raw_digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn publish(
    private: &Path,
    public: &Path,
    mut queue: Vec<references::Reference>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    while let Some(reference) = queue.pop() {
        if !seen.insert((reference.digest.clone(), reference.json)) {
            continue;
        }
        let source = private.join(&reference.digest);
        if raw_digest(&source)? != reference.digest {
            let manifest: Value = serde_json::from_slice(&fs::read(&source)?)?;
            if manifest["kah_source_segments"] != 1 || manifest["digest"] != reference.digest {
                return Err("REVIEW_SOURCE_DIGEST stage=source_view".into());
            }
            for block in manifest["blocks"]
                .as_array()
                .ok_or("REVIEW_SOURCE_SEGMENTS stage=source_view")?
            {
                let id = block.as_str().ok_or("REVIEW_SOURCE_ID stage=source_view")?;
                if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("REVIEW_SOURCE_ID stage=source_view".into());
                }
                let name = format!("block-{id}");
                let path = private.join(&name);
                if raw_digest(&path)? != id {
                    return Err("REVIEW_SOURCE_DIGEST stage=source_view".into());
                }
                independent(&path, &public.join(name))?;
            }
        }
        let reader = super::super::segments::reader::open(&source, &reference.digest)?;
        if reference.json {
            queue.extend(references::collect(reader)?);
        } else {
            std::io::copy(&mut { reader }, &mut std::io::sink())?;
        }
        independent(&source, &public.join(&reference.digest))?;
    }
    fs::File::open(public)?.sync_all()?;
    Ok(())
}
fn independent(source: &Path, target: &Path) -> Result<()> {
    if let Ok(saved) = fs::symlink_metadata(target)
        && saved.is_file()
        && saved.nlink() == 1
    {
        return Ok(());
    }
    let staging = target
        .parent()
        .unwrap()
        .join(format!("{}.tmp", crate::relay_store::nonce()));
    fs::copy(source, &staging)?;
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o600))?;
    fs::File::open(&staging)?.sync_all()?;
    fs::rename(staging, target)?;
    Ok(())
}
