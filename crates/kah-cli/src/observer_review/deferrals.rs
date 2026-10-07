//! Controller-owned identities bind a requirement and its named dependency.
use super::state::Request;
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub fn collect(request: &Request, actions: &[Value]) -> Result<Vec<Value>> {
    super::failure::validation(super::failure::Origin::ReportValidation, || {
        validate(request, actions)
    })
}
fn validate(request: &Request, actions: &[Value]) -> Result<Vec<Value>> {
    let mut items = request.intent["open_deferrals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for action in actions
        .iter()
        .filter(|a| a["evidence"]["topic"] == "deferral")
    {
        let evidence = &action["evidence"];
        let declaration:kah_observer::review_evidence::Deferral=serde_json::from_value(evidence["deferral"].clone()).map_err(|_|"REVIEW_DEFERRAL_METADATA stage=checkpoint: cite the requirement, named dependency phase and existing id when revisiting it")?;
        let requirement = evidence["citations"]
            .get(declaration.requirement_citation)
            .ok_or("REVIEW_DEFERRAL_CITATION stage=checkpoint")?;
        let dependency = evidence["citations"]
            .get(declaration.dependency_citation)
            .ok_or("REVIEW_DEFERRAL_CITATION stage=checkpoint")?;
        let id = if let Some(id) = &declaration.id {
            let Some(existing) = items.iter().find(|i| i["id"] == *id) else {
                return Err(
                    "REVIEW_DEFERRAL_UNKNOWN stage=checkpoint: only the controller assigns new ids"
                        .into(),
                );
            };
            if existing["phase"] != declaration.phase
                || existing["requirement"]["uri"] != requirement["uri"]
                || existing["requirement"]["quote"] != requirement["quote"]
            {
                return Err("REVIEW_DEFERRAL_REDEFINED stage=checkpoint: keep the original requirement and named phase on this controller id".into());
            }
            id.clone()
        } else {
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&json!([
                    requirement["uri"],
                    requirement["quote"],
                    declaration.phase
                ]))?)
            )
        };
        let update = json!({"id":id,"phase":declaration.phase,"requirement":requirement,"dependency":dependency,"evidence":evidence,"introduced_at":request.id});
        if let Some(old) = items.iter_mut().find(|i| i["id"] == id) {
            let introduced = old["introduced_at"].clone();
            *old = update;
            old["introduced_at"] = introduced;
        } else {
            items.push(update)
        }
    }
    Ok(items)
}
