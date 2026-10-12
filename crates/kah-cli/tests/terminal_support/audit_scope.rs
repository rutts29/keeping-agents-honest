//! An explicit owner scope never forgives an unattributed fixture resource.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Clone, Serialize, Deserialize)]
pub struct Resource {
    pub identity: Value,
    pub paths: Vec<PathBuf>,
    pub owner: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Scope {
    pub owner: String,
    pub roots: Vec<PathBuf>,
    pub active_foreign: Vec<PathBuf>,
}
pub fn report(scope: &Scope, resources: Vec<Resource>) -> Value {
    let mut own = vec![];
    let mut foreign = vec![];
    let mut unknown = vec![];
    for resource in resources {
        if resource.owner.as_deref() == Some(scope.owner.as_str())
            || resource
                .paths
                .iter()
                .any(|p| scope.roots.iter().any(|root| p.starts_with(root)))
        {
            own.push(resource.identity);
        } else if let Some(root) = scope
            .active_foreign
            .iter()
            .find(|root| resource.paths.iter().any(|p| p.starts_with(root)))
        {
            foreign.push(json!({"resource":resource.identity,"fixture_root":root,"status":"foreign, concurrent, attributed"}));
        } else {
            unknown.push(resource.identity);
        }
    }
    json!({"passed":own.is_empty()&&unknown.is_empty(),"own":own,"unattributed":unknown,"foreign_concurrent_attributed":foreign})
}
