//! Narrow checked-row context: rule flags and whole cited files, not a decision Input.
use crate::Result;
use kah_observer::commit_check::{CodeFile, FindingKind, Row, Status, verify_file_quote};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
struct Flags {
    prohibition: bool,
}
#[derive(Deserialize)]
struct Scope {
    revision: String,
    rules: BTreeMap<String, Flags>,
}
/// Every rule ID with its prohibition flag, plus each whole file a row cites.
pub struct RowScope {
    revision: String,
    prohibitions: BTreeMap<String, bool>,
    files: BTreeMap<String, CodeFile>,
}
impl RowScope {
    /// Reads the complete job input or its selected parts with identical results.
    pub fn read(input: &Value, rows: &[Row]) -> Result<Self> {
        let scope = Scope::deserialize(input)?;
        let mut files = BTreeMap::new();
        for quote in rows.iter().flat_map(|row| &row.quotes) {
            if let Some(file) = input["files"].get(&quote.file)
                && !files.contains_key(&quote.file)
            {
                files.insert(quote.file.clone(), CodeFile::deserialize(file)?);
            }
        }
        Ok(Self {
            revision: scope.revision,
            prohibitions: scope
                .rules
                .into_iter()
                .map(|(id, flags)| (id, flags.prohibition))
                .collect(),
            files,
        })
    }
}
pub fn validate_rows(scope: &RowScope, rows: &[Row]) -> Result<()> {
    let expected = scope.prohibitions.keys().collect::<BTreeSet<_>>();
    let actual = rows.iter().map(|r| &r.requirement).collect::<BTreeSet<_>>();
    if expected != actual || rows.len() != actual.len() {
        return Err("COMMIT_REPORT_SCOPE stage=report".into());
    }
    for row in rows {
        if row.requirement == "controller_intent_pending" && row.status != Status::Unclear {
            return Err(
                "COMMIT_INTENT_NOT_ACCEPTED stage=report: startup intent stays unconfirmed".into(),
            );
        }
        if row.kind == FindingKind::NotViolated && !scope.prohibitions[&row.requirement] {
            return Err("COMMIT_REPORT_ABSENCE_SCOPE stage=report".into());
        }
        if row.status == Status::Violated && row.quotes.is_empty() {
            return Err("COMMIT_REPORT_QUOTES stage=report".into());
        }
        for quote in &row.quotes {
            verify_file_quote(&scope.revision, &scope.files, quote)?;
        }
    }
    Ok(())
}
