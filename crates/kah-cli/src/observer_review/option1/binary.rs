//! Native Git bytes are stored once; the observer receives only explicit descriptors.
use super::super::{
    source_store,
    state::{State, Store},
};
use crate::Result;
use serde_json::{Value, json};
pub fn present(context: &Value) -> bool {
    context["touched_files"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|f| matches!(f["content"]["encoding"].as_str(), Some("bytes" | "binary")))
}
pub fn publish(store: &Store, state: &State, context: &mut Value) -> Result<()> {
    for item in context["touched_files"]
        .as_array_mut()
        .ok_or("COMMIT_FILES_MISSING stage=binary")?
    {
        if item["content"]["encoding"] != "bytes" {
            continue;
        }
        let bytes: Vec<u8> = serde_json::from_value(item["content"]["bytes"].clone())?;
        let source = source_store::binary::store(&store.directory, &bytes)?;
        let patch = item["patch"]
            .as_str()
            .ok_or("COMMIT_PATCH_MISSING stage=binary")?;
        let patch_source = source_store::binary::store(&store.directory, patch.as_bytes())?;
        let digest = source["$kah_source"]
            .as_str()
            .ok_or("REVIEW_SOURCE_ID stage=binary")?;
        item["content"] = json!({"encoding":"binary","path":item["path"],"size":bytes.len(),
            "sha256":digest,"git_blob":item["blob"],"deleted":item["deleted"],"source":source,"git_diff_source":patch_source,
            "read_path":state.observer.workspace.join("sources").join(digest),
            "read_command":"bin/kah observe read-blob READ_PATH > outbox/binary-source"});
        item["patch"] = json!(
            "Binary Git source: complete bytes and original binary patch are available through the lossless CAS descriptors."
        );
    }
    Ok(())
}
pub fn descriptors(context: &Value) -> Value {
    let mut descriptors = serde_json::Map::new();
    for item in context["touched_files"].as_array().into_iter().flatten() {
        if item["content"]["encoding"] == "binary"
            && let Some(path) = item["path"].as_str()
        {
            descriptors.insert(path.into(), item["content"].clone());
        }
    }
    Value::Object(descriptors)
}
/// A binary commit cannot acquire a Jev-only textual pass.
pub fn wake(
    store: Store,
    state: State,
    home: &std::path::Path,
    turn: &str,
    input: kah_observer::commit_check::Input,
    trigger: Value,
) -> Result<Value> {
    let head = input.revision.clone();
    let mut record = kah_observer::commit_review::classify(
        &state.worker.session,
        &head,
        &[],
        &super::journal::policy(home)?,
    )?;
    record.gaps.push(
        json!({"route":"direct_observer_review","reason":"binary_git_sources",
        "sources":input.carried["binary_sources"],"full_sources_retained":true}),
    );
    super::journal::save(&record)?;
    let plan = if input.checkpoint {
        json!(kah_observer::commit_review::checkpoint::plan(
            &super::journal::all(&state.worker.session)?,
            &kah_observer::commit_review::authority(&input),
            &input
                .files
                .iter()
                .map(|(p, f)| (p.clone(), f.digest.clone()))
                .collect()
        ))
    } else {
        Value::Null
    };
    super::native::submit(
        store,
        state,
        home,
        turn,
        &head,
        json!({"stage":if input.checkpoint{"checkpoint"}else{"confirmation"},"input":input,
            "record":record,"plan":plan,"original_trigger":trigger,
            "direct_reason":"Binary Git blobs are lossless CAS descriptors, not UTF8 code. Read complete bytes on demand using read-blob; no binary bytes were inlined or cut. Review their meaning and the whole retained diff using native evidence."}),
        trigger.clone(),
    )
}
