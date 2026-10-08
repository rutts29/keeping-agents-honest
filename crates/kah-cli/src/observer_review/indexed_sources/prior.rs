//! serde_json 1.0.151 src/raw.rs: RawValue retains exact formatting, unlike Value.
use super::*;
use serde_json::value::RawValue;
use std::collections::BTreeMap;
fn field<'a>(text: &'a str, key: &str) -> Result<Option<&'a str>> {
    let object: BTreeMap<String, &'a RawValue> = serde_json::from_str(text)?;
    Ok(object.get(key).map(|raw| raw.get()))
}
pub(super) fn publish(private: &Path, directory: &Path, doc: &Document) -> Result<Vec<Value>> {
    let report = field(&doc.text, "report")?.unwrap_or(&doc.text);
    let mut groups = vec![];
    if let Some(units) = field(report, "decision_units")?
        && units != "null"
        && let Some(requirements) = field(units, "requirements")?
    {
        groups.push(("requirements", requirements));
    }
    if let Some(findings) = field(report, "findings")? {
        groups.push(("findings", findings));
    }
    let mut records = vec![];
    for (group, text) in groups {
        let values: Vec<&RawValue> = serde_json::from_str(text)?;
        for (index, value) in values.iter().enumerate() {
            let raw = value.get();
            let start = doc
                .text
                .find(raw)
                .ok_or("REVIEW_RECORD_RANGE stage=index")?;
            let path = directory.join(format!("{group}-{index}.txt"));
            source_store::text(private, &path, raw)?;
            let unit: Value = serde_json::from_str(raw)?;
            records.push(json!({"group":group,"record":index,"id":unit["id"],
                "name":unit["name"],"topic":unit["topic"],"path":path,
                "start_byte":start,"end_byte":start+raw.len(),"bytes":raw.len(),
                "source":doc.id,"source_digest":doc.digest}));
        }
    }
    Ok(records)
}
