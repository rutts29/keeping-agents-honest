//! Whole Git patch records. Irreducible records route to direct native review.
use crate::Result;
use kah_observer::{
    context_selection::digest,
    whole_preparation::{Catalogue, Record, Source},
};
use serde_json::Value;
use std::collections::BTreeSet;
pub fn catalogue(body: &Value) -> Result<Catalogue> {
    let mut sources = vec![];
    let hunks = body["state"]["diff_hunks"]
        .as_object()
        .ok_or("COMMIT_HUNKS_MISSING stage=preparation")?;
    for (id, hunk) in hunks {
        let document = hunk["text"].clone();
        if !document.is_string() {
            return Err("COMMIT_PATCH_TYPE stage=preparation".into());
        }
        let hash = digest(&document);
        sources.push(Source {
            target: format!("/state/diff_hunks/{id}/text"),
            original_digest: hash.clone(),
            document,
            document_digest: hash.clone(),
            records: vec![Record {
                id: format!("patch-{id}"),
                pointer: "".into(),
                digest: hash,
                dependencies: BTreeSet::new(),
                required: false,
            }],
        });
    }
    Ok(Catalogue {
        body_digest: digest(body),
        sources,
    })
}
