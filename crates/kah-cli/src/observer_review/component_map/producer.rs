//! A map is checked metadata, never an authority item or an overall verdict.
use super::Map;
use crate::{
    Result,
    observer_review::{
        source_store,
        state::{Request, Store},
    },
};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub fn path(workspace: &Path) -> PathBuf {
    workspace.join("component-map-data/current.json")
}
pub fn validate(request: &Request, report: &Report) -> Result<Option<Map>> {
    let Some(value) = &report.component_map else {
        return Ok(None);
    };
    if report.request_id != request.id {
        return Err(
            "REVIEW_MAP_BINDING stage=component_map: proposal belongs to another request".into(),
        );
    }
    if !crate::observer_review::commit_packet::is_commit(request) {
        return Err(
            "REVIEW_MAP_CLASS stage=component_map: declarations belong to light commit reports"
                .into(),
        );
    }
    let map: Map = serde_json::from_value(value.clone())
        .map_err(|e| format!("REVIEW_MAP_JSON stage=component_map: {e}"))?;
    let requirements = if report.decision_units.is_some() {
        crate::observer_review::verdict_cache::integration::units(request, report)?
            .requirements
            .iter()
            .map(|r| r.id.clone())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    map.validate(&requirements)?;
    Ok(Some(map))
}
pub fn record(store: &Store, request: &Request, workspace: &Path, report: &Report) -> Result<()> {
    let Some(map) = validate(request, report)? else {
        return Ok(());
    };
    let root = store.directory.join("component-maps");
    let digest = map.digest()?;
    kah_core::run_store::ensure_private_directory(&root)?;
    source_store::save(
        &root,
        &format!("{}.json", request.id),
        &json!({"request":request.id,"worker":request.worker,"head":request.head,"map":map,"map_digest":digest,
            "status":"declaration checked; runtime dependency validation still required"}),
    )?;
    let target = path(workspace);
    let public = target.parent().unwrap();
    kah_core::run_store::ensure_private_directory(public)?;
    crate::unattended_report::save(public, "current.json", &serde_json::to_value(map)?)?;
    kah_run::access::share_tree(public, kah_run::access::service_uid()?, false)?;
    source_store::save(
        &root,
        "current.json",
        &json!({"request":request.id,"worker":request.worker,
        "head":request.head,"map_digest":digest}),
    )?;
    Ok(())
}
pub fn current(directory: &Path, worker: &str) -> Result<Option<Value>> {
    let root = directory.join("component-maps");
    let Some(stamp) = crate::observer_review::state::read::<Value>(&root.join("current.json"))?
    else {
        return Ok(None);
    };
    if stamp["worker"] != worker {
        return Err(
            "REVIEW_MAP_BINDING stage=component_map: different worker in private pointer".into(),
        );
    }
    let request = stamp["request"]
        .as_str()
        .ok_or("REVIEW_MAP_BINDING stage=component_map: missing request")?;
    if !request.starts_with("review-")
        || !request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(
            "REVIEW_MAP_BINDING stage=component_map: invalid private request reference".into(),
        );
    }
    let original: Value =
        crate::observer_review::state::read(&root.join(format!("{request}.json")))?
            .ok_or("REVIEW_MAP_BINDING stage=component_map: private original missing")?;
    let map: Map = serde_json::from_value(original["map"].clone())?;
    if original["worker"] != worker
        || original["request"] != request
        || original["map_digest"] != stamp["map_digest"]
        || map.digest()? != stamp["map_digest"]
    {
        return Err("REVIEW_MAP_BINDING stage=component_map: private provenance mismatch".into());
    }
    Ok(Some(stamp))
}
pub fn check_collected(stamp: &Value, project: &Value) -> Result<()> {
    if stamp["map_digest"].is_null()
        || stamp["map_digest"] != project["component_impact"]["declaration_digest"]
    {
        return Err("REVIEW_MAP_BINDING stage=component_map: collected declaration differs from private checked proposal".into());
    }
    Ok(())
}
pub fn schema() -> Value {
    let check = json!({"type":"object","additionalProperties":false,"required":["name","command"],
        "properties":{"name":{"type":"string"},"command":{"type":"string"}}});
    let strings = json!({"type":"array","items":{"type":"string"}});
    json!({"type":"object","additionalProperties":false,"required":["version","components"],
        "properties":{"version":{"const":1},"components":{"type":"array","items":{
            "type":"object","additionalProperties":false,"required":["name","path_globs","requirements","tests","e2e_flows"],
            "properties":{"name":{"type":"string"},"path_globs":strings,"requirements":strings,
                "tests":{"type":"array","items":check},"e2e_flows":{"type":"array","items":check}}}}}})
}
