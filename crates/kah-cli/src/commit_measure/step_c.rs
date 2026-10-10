//! Existing C owns evidence validation/cache eligibility; this bridge only scopes.
use super::*;
use crate::observer_review::{state::Request, verdict_cache};
pub fn apply(private: &Path, request: &mut Request, bundle: &mut Bundle) -> Result<()> {
    verdict_cache::plan::prepare(private, request)?;
    let plan = &request.trigger["cache_plan"];
    bundle.input.impacted.extend(
        plan["impacted_requirements"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    bundle.input.impacted.extend(
        plan["dirty"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    bundle.input.carried = json!({"protocol":plan["protocol"],"records":plan["carried"],
        "source":plan["record_source"],"digest":plan["record_digest"],"unattributed_paths":plan["unattributed_paths"]});
    // Always-set prohibitions remain selected by commit_check, regardless of C.
    Ok(())
}
