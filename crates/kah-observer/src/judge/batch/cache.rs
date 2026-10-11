use super::*;
use sha2::{Digest, Sha256};
pub(super) fn key(request: &WireRequest) -> String {
    format!(
        "{:x}",
        Sha256::digest([request.id.as_bytes(), b"\0", request.body.as_bytes()].concat())
    )
}
pub(super) fn load(root: &Path, request: &WireRequest) -> Result<Option<Receipt>> {
    let receipt: Option<Receipt> = store::read(&root.join(format!("{}.json", key(request))))?;
    if receipt
        .as_ref()
        .is_some_and(|r| r.request.id != request.id || r.request.body != request.body)
    {
        return Err(Error::new("JEV_BATCH_CACHE_BINDING"));
    }
    Ok(receipt)
}
pub(super) fn save(
    root: &Path,
    request: &WireRequest,
    response: &RawResponse,
    elapsed_ms: u64,
) -> Result<Receipt> {
    let receipt = Receipt {
        request: request.clone(),
        response: response.clone(),
        reused: false,
        elapsed_ms,
    };
    store::write(&root.join(format!("{}.json", key(request))), &receipt)?;
    Ok(receipt)
}
