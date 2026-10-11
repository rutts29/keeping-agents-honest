//! Classification is immutable per native record, independent of intent lifecycle.
use super::Store;
use kah_core::{
    intent_history::SourceRef,
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader},
};

pub const CLASSIFIER_VERSION: u32 = 2;

#[derive(Clone, Deserialize, Serialize)]
struct Classification {
    classifier_version: u32,
    source: SourceRef,
}
impl Store {
    /// Restore stored roles only for still-present, digest-identical native records.
    /// New records keep the provider's current class; no source text is rewritten.
    pub fn classified_users(
        &mut self,
        native: &NeutralSessionBundle,
    ) -> Result<Option<NeutralSessionBundle>, String> {
        let prior = self.classifications()?;
        let mut fresh = vec![];
        let mut records = vec![];
        let present = native
            .records()
            .iter()
            .map(BundleRecord::id)
            .collect::<BTreeSet<_>>();
        for (record, source) in native.records().iter().zip(native.source_refs()) {
            let class = prior.get(record.id()).cloned().unwrap_or_else(|| {
                let class = Classification {
                    classifier_version: CLASSIFIER_VERSION,
                    source,
                };
                fresh.push(class.clone());
                class
            });
            let restored =
                BundleRecord::new(record.id(), class.source.role.clone(), record.content())
                    .map_err(|e| e.to_string())?;
            if class.source.provider != native.source().provider()
                || class.source.session != native.source().session_ref_private()
                || restored.content_digest() != class.source.content_digest
            {
                return Err("OBSERVER_SOURCE_REWRITE stage=classification".into());
            }
            if restored.is_authoritative() {
                records.push(restored);
            }
        }
        if prior.keys().any(|id| !present.contains(id.as_str())) {
            return Err(
                "OBSERVER_SOURCE_REWRITE stage=classification: native record missing".into(),
            );
        }
        for class in fresh {
            self.append(&json!({"kind":"source_classification_v1","classifier_version":class.classifier_version,"source":class.source}))?;
        }
        if records.is_empty() {
            return Ok(None);
        }
        NeutralSessionBundle::from_records_with_native_goal(
            native.source().provider(),
            native.source().session_ref_private(),
            native.source().repository_digest(),
            native.native_goal().clone(),
            records,
        )
        .map(Some)
        .map_err(|e| e.to_string())
    }
    fn classifications(&self) -> Result<BTreeMap<String, Classification>, String> {
        let mut classes = BTreeMap::new();
        // Existing source-role fields are the v1 classification. This also
        // validates the journal's private-file and source-authority invariants.
        for input in self
            .sources()?
            .into_iter()
            .filter(|i| i.local_record.is_none())
        {
            classes.insert(
                input.key().to_owned(),
                Classification {
                    classifier_version: 1,
                    source: input.source,
                },
            );
        }
        let file = match fs::File::open(self.path.join("observations.jsonl")) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(classes),
            Err(e) => return Err(format!("OBSERVER_SOURCE_IO stage=classification: {e}")),
        };
        for line in BufReader::new(file).lines() {
            let value: Value = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let class = match value["kind"].as_str() {
                Some("source_classification_v1") => serde_json::from_value::<Classification>(value)
                    .map_err(|e| {
                        format!("OBSERVER_CLASSIFICATION_SCHEMA stage=classification: {e}")
                    })?,
                Some("user_source_v1") if value["input"]["local_record"].is_null() => {
                    Classification {
                        classifier_version: serde_json::from_value(
                            value.get("classifier_version").cloned().unwrap_or(json!(1)),
                        )
                        .map_err(|_| "OBSERVER_CLASSIFICATION_VERSION stage=classification")?,
                        source: serde_json::from_value(value["input"]["source"].clone())
                            .map_err(|_| "OBSERVER_CLASSIFICATION_SCHEMA stage=classification")?,
                    }
                }
                _ => continue,
            };
            if !(1..=CLASSIFIER_VERSION).contains(&class.classifier_version) {
                return Err("OBSERVER_CLASSIFICATION_VERSION stage=classification".into());
            }
            if let Some(old) = classes.get(&class.source.record)
                && old.source != class.source
            {
                return Err(
                    "OBSERVER_SOURCE_REWRITE stage=classification: stored class changed".into(),
                );
            }
            classes.insert(class.source.record.clone(), class);
        }
        Ok(classes)
    }
}
