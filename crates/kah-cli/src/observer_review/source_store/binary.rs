//! Binary content remains a lossless reference; it is never expanded into prompts.
use super::*;
pub fn store(private: &Path, bytes: &[u8]) -> Result<Value> {
    reference(&private.join("sources"), bytes, "binary")
}
pub(super) fn verify(root: &Path, digest: &str) -> Result<()> {
    let path = resolved(&root.join(digest), digest);
    let mut reader = segments::reader::open(&path, digest)?;
    std::io::copy(&mut reader, &mut std::io::sink())?;
    Ok(())
}
pub fn print(path: &Path) -> Result<()> {
    let digest = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("REVIEW_SOURCE_ID stage=binary")?;
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("REVIEW_SOURCE_ID stage=binary".into());
    }
    let path = resolved(path, digest);
    let mut reader = segments::reader::open(&path, digest)?;
    std::io::copy(&mut reader, &mut std::io::stdout())?;
    Ok(())
}
