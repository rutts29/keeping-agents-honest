//! Late failures remain evidence; they cannot become the next request's gap.
use super::*;
pub fn save(store: &Store, gap: &Value) -> Result<bool> {
    let current = store.load()?.and_then(|s| s.pending).map(|p| p.id);
    if gap["request"]
        .as_str()
        .is_some_and(|id| Some(id) == current.as_deref())
    {
        crate::unattended_report::save(&store.directory, "gap.json", gap)?;
        return Ok(true);
    }
    let root = store.directory.join("retired-gaps");
    kah_core::run_store::ensure_private_directory(&root)?;
    crate::unattended_report::save(
        &root,
        &format!("{}.json", digest(&serde_json::to_vec(gap)?)),
        &json!({"gap":gap,"current_request":current,"retired_ms":kah_observer::session::now_ms(),"reason":"not_current_request"}),
    )?;
    Ok(false)
}
