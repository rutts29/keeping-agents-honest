//! Host expansion binds the rejected report to its captured request and digest.
use super::*;
use crate::observer_review::{
    source_store,
    state::{State, Store},
};
pub fn rejected(store: &Store, state: &State, request: &Request) -> Result<Request> {
    let mut expanded = request.clone();
    if !is_scoped(request) {
        return Ok(expanded);
    }
    if state.worker.session != request.worker
        || store.directory != Store::directory(&request.worker)?
    {
        return Err("REVIEW_EXPANSION_BINDING stage=packet".into());
    }
    let Some(record) = crate::observer_review::state::read::<Value>(
        &store.directory.join("validation-current.json"),
    )?
    else {
        return Ok(expanded);
    };
    let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(request)?));
    if record["request"] != request.id || record["request_digest"] != digest {
        return Ok(expanded);
    }
    let path = std::path::PathBuf::from(
        record["report"]
            .as_str()
            .ok_or("REVIEW_EXPANSION_REPORT stage=packet")?,
    );
    let bytes = super::read::report(&state.observer.workspace, &path)?;
    if record["report_digest"] != format!("{:x}", Sha256::digest(&bytes)) {
        return Ok(expanded);
    }
    let added = expand(&mut expanded, &serde_json::from_slice(&bytes)?)?;
    if added.is_empty() {
        return Ok(expanded);
    }
    let archive = store.directory.join("scope-expansions");
    kah_core::run_store::ensure_private_directory(&archive)?;
    source_store::save(
        &archive,
        &format!("{}-{}.json", request.id, crate::relay_store::nonce()),
        &json!({"original_request":request,"added":added,"report_digest":record["report_digest"],"whole_files":true,"at_ms":kah_observer::session::now_ms()}),
    )?;
    expanded.trigger["source_expansions"] = json!(added);
    Ok(expanded)
}
