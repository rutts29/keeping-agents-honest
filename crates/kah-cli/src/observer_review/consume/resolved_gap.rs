//! Successful evaluation resolves only diagnostics bound to that same request.
use crate::Result;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn archive(root: &Path, request: &str) -> Result<()> {
    for name in ["gap.json", "delivered-gap.json", "validation-current.json"] {
        let path = root.join(name);
        let Some(value) = super::super::state::read::<Value>(&path)? else {
            continue;
        };
        if value["request"] != request {
            continue;
        }
        let archive = root.join("resolved-gaps");
        kah_core::run_store::ensure_private_directory(&archive)?;
        super::super::source_store::save(
            &archive,
            &format!("{request}-{name}-{}.json", crate::relay_store::nonce()),
            &json!({"request":request,"resolved_by":"successful_consume","source_file":name,"original":value,"at_ms":kah_observer::session::now_ms()}),
        )?;
        fs::remove_file(&path).map_err(|error| {
            format!(
                "REVIEW_GAP_RESOLVE_IO stage=consume_completion: {}: {error}",
                path.display()
            )
        })?;
    }
    Ok(())
}
