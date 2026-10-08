//! Thinker-maintained impact declarations, validated against native Git.
//! Commands are check proposals, never executed on the host by this module.
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
mod native;
pub mod producer;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Map {
    pub version: u32,
    pub components: Vec<Component>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub name: String,
    pub path_globs: Vec<String>,
    pub requirements: Vec<String>,
    pub tests: Vec<Check>,
    pub e2e_flows: Vec<Check>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub name: String,
    pub command: String,
}
impl Map {
    pub fn validate(&self, requirements: &[String]) -> Result<()> {
        self.structure()?;
        for component in &self.components {
            for id in &component.requirements {
                if !requirements.contains(id) {
                    return Err(format!("REVIEW_MAP_REQUIREMENT stage=component_map: component {:?} links unknown requirement {id:?}", component.name).into());
                }
            }
        }
        Ok(())
    }
    fn structure(&self) -> Result<()> {
        if self.version != 1 {
            return Err("REVIEW_MAP_VERSION stage=component_map".into());
        }
        let mut names = BTreeSet::new();
        for component in &self.components {
            if component.name.trim().is_empty() || !names.insert(&component.name) {
                return Err(format!(
                    "REVIEW_MAP_COMPONENT stage=component_map: empty or duplicate {:?}",
                    component.name
                )
                .into());
            }
            if component.path_globs.is_empty() {
                return Err(format!(
                    "REVIEW_MAP_PATH stage=component_map: {:?} has no path_globs",
                    component.name
                )
                .into());
            }
            for pattern in &component.path_globs {
                path(pattern)?;
            }
            if component.requirements.iter().any(|id| id.trim().is_empty())
                || component
                    .tests
                    .iter()
                    .chain(&component.e2e_flows)
                    .any(|c| c.name.trim().is_empty() || c.command.trim().is_empty())
            {
                return Err(format!(
                    "REVIEW_MAP_CHECK stage=component_map: {:?} has an empty requirement/check",
                    component.name
                )
                .into());
            }
        }
        Ok(())
    }
    pub fn collect(&self, root: &Path, head: &str, baseline: Option<&str>) -> Result<Value> {
        self.structure()?;
        native::sha(head)?;
        let empty = native::empty(root)?;
        let base = baseline.unwrap_or(&empty);
        native::sha(base)?;
        let changed = native::paths(root, base, head, &[])?;
        let inventory = native::inventory(root, head)?;
        let mut mapped = BTreeSet::new();
        let mut components = vec![];
        for component in &self.components {
            let touched = native::paths(root, base, head, &component.path_globs)?;
            let selected = native::paths(root, &empty, head, &component.path_globs)?;
            let files: BTreeMap<_, _> = selected
                .iter()
                .chain(&touched)
                .map(|path| (path.clone(), inventory.get(path).cloned()))
                .collect();
            mapped.extend(touched.iter().cloned());
            let current_files: BTreeMap<_, _> = selected
                .iter()
                .map(|path| (path, inventory.get(path)))
                .collect();
            let digest = format!(
                "sha256:{:x}",
                Sha256::digest(serde_json::to_vec(
                    &json!({"declaration":component,"files":current_files})
                )?)
            );
            components.push(json!({"name":component.name,"path_globs":component.path_globs,"digest":digest,"files":files,
                "changed_paths":touched,"requirements":component.requirements,
                "tests":component.tests,"e2e_flows":component.e2e_flows}));
        }
        let unmapped: Vec<_> = changed.difference(&mapped).cloned().collect();
        Ok(
            json!({"version":1,"head":head,"baseline":baseline,"components":components,
            "unmapped_paths":unmapped,"cochange_history":native::history(root, head, baseline)?,
            "runtime_read_validation":"not yet captured","untouched_test_failures":"not yet captured"}),
        )
    }
}
fn path(pattern: &str) -> Result<()> {
    if pattern.is_empty()
        || pattern.contains('\0')
        || pattern.starts_with('/')
        || pattern.split('/').any(|p| matches!(p, "." | ".."))
    {
        return Err(
            format!("REVIEW_MAP_PATH stage=component_map: unsafe path_glob {pattern:?}").into(),
        );
    }
    Ok(())
}
