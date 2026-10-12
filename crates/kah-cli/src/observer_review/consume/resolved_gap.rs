//! Successful evaluation resolves only diagnostics bound to that same request.
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
        kah_core::run_store::ensure_private_directory(&root.join("sources"))?;
        let archive_name = format!("{request}-{name}-{}.json", crate::relay_store::nonce());
        let original = fs::read_to_string(&path)?;
        let original_record = archive.join(format!("{archive_name}.original"));
        super::super::source_store::text(root, &original_record, &original)?;
        if super::super::source_store::load_text(&original_record)? != original {
            return Err("REVIEW_GAP_RESOLVE_DIGEST stage=consume_completion".into());
        }
        super::super::source_store::save(
            &archive,
            &archive_name,
            &json!({"request":request,"resolved_by":"successful_consume","source_file":name,"original":value,"original_record":original_record,"original_sha256":format!("{:x}",Sha256::digest(original.as_bytes())),"at_ms":kah_observer::session::now_ms()}),
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
