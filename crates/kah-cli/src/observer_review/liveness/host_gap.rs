use super::*;
pub(super) fn host_failure(root: &Path, request: &str) -> Result<Option<String>> {
    let gap: Option<Value> = crate::observer_review::state::read(&root.join("gap.json"))?;
    Ok(gap
        .filter(|g| {
            g["request"].as_str().is_some_and(|id| id == request)
                && (crate::observer_review::failure::correctable(
                    serde_json::from_value(g["origin"].clone()).unwrap_or_default(),
                    g["error"].as_str().unwrap_or(""),
                ) || !crate::observer_review::redelivery::gap_retryable(g))
        })
        .and_then(|g| g["error"].as_str().map(str::to_owned)))
}
