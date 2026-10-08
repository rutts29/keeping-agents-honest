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
    let requirements = report
        .decision_units
        .iter()
        .flat_map(|units| &units.requirements)
        .map(|r| r.id.clone())
        .collect::<Vec<_>>();
    map.validate(&requirements)?;
    Ok(Some(map))
}
pub fn record(store: &Store, request: &Request, workspace: &Path, report: &Report) -> Result<()> {
    let Some(map) = validate(request, report)? else {
        return Ok(());
    };
    let root = store.directory.join("component-maps");
    kah_core::run_store::ensure_private_directory(&root)?;
    source_store::save(
        &root,
        &format!("{}.json", request.id),
        &json!({"request":request.id,"worker":request.worker,"head":request.head,"map":map,
            "status":"declaration checked; runtime dependency validation still required"}),
    )?;
    let target = path(workspace);
    let public = target.parent().unwrap();
    kah_core::run_store::ensure_private_directory(public)?;
    crate::unattended_report::save(public, "current.json", &serde_json::to_value(map)?)?;
    kah_run::access::share_tree(public, kah_run::access::service_uid()?, false)?;
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
