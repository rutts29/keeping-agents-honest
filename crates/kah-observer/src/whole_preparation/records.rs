use super::*;
pub(super) fn validate<'a>(
    body: &Value,
    catalogue: &'a Catalogue,
) -> Result<BTreeMap<&'a str, &'a Record>, String> {
    if digest(body) != catalogue.body_digest {
        return Err("PREPARATION_BODY_BINDING stage=whole_preparation".into());
    }
    let targets = body["state"]["diff_hunks"]
        .as_object()
        .ok_or("PREPARATION_HUNKS stage=whole_preparation")?;
    let expected = targets
        .keys()
        .map(|key| format!("/state/diff_hunks/{}/text", escape(key)))
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut known = BTreeMap::new();
    for source in &catalogue.sources {
        if !expected.contains(&source.target)
            || !seen.insert(source.target.clone())
            || body
                .pointer(&source.target)
                .is_none_or(|v| digest(v) != source.original_digest)
            || digest(&source.document) != source.document_digest
        {
            return Err("PREPARATION_SOURCE_BINDING stage=whole_preparation".into());
        }
        validate_source(source, &mut known)?;
    }
    if seen != expected {
        return Err("PREPARATION_SOURCE_ACCOUNTING stage=whole_preparation".into());
    }
    if known.values().any(|r| {
        r.dependencies
            .iter()
            .any(|d| !known.contains_key(d.as_str()))
    }) {
        return Err("PREPARATION_UNKNOWN_DEPENDENCY stage=whole_preparation".into());
    }
    Ok(known)
}
fn validate_source<'a>(
    source: &'a Source,
    known: &mut BTreeMap<&'a str, &'a Record>,
) -> Result<(), String> {
    let mut pointers = BTreeSet::new();
    for record in &source.records {
        let value = source
            .document
            .pointer(&record.pointer)
            .ok_or("PREPARATION_RECORD_POINTER stage=whole_preparation")?;
        if record.id.is_empty()
            || known.insert(&record.id, record).is_some()
            || digest(value) != record.digest
            || pointers.iter().any(|p: &&str| overlaps(p, &record.pointer))
        {
            return Err("PREPARATION_RECORD_BINDING stage=whole_preparation".into());
        }
        pointers.insert(record.pointer.as_str());
    }
    if !covered(&source.document, "", &pointers) {
        return Err("PREPARATION_CATALOGUE_COVERAGE stage=whole_preparation".into());
    }
    Ok(())
}
fn overlaps(a: &str, b: &str) -> bool {
    a == b
        || a.is_empty()
        || b.is_empty()
        || a.starts_with(&format!("{b}/"))
        || b.starts_with(&format!("{a}/"))
}
fn covered(value: &Value, pointer: &str, records: &BTreeSet<&str>) -> bool {
    if records.contains(pointer) {
        return true;
    }
    match value {
        Value::Object(map) if !map.is_empty() => map
            .iter()
            .all(|(k, v)| covered(v, &format!("{pointer}/{}", escape(k)), records)),
        Value::Array(values) if !values.is_empty() => values
            .iter()
            .enumerate()
            .all(|(i, v)| covered(v, &format!("{pointer}/{i}"), records)),
        _ => false,
    }
}
pub(super) fn render(
    body: &Value,
    catalogue: &Catalogue,
    selected: &BTreeSet<&str>,
) -> Result<Value, String> {
    let mut result = body.clone();
    for source in &catalogue.sources {
        // Keep-all is byte-content-preserving; it cannot evade a rejection receipt.
        if source
            .records
            .iter()
            .all(|r| selected.contains(r.id.as_str()))
        {
            continue;
        }
        let values = source
            .records
            .iter()
            .filter(|r| selected.contains(r.id.as_str()))
            .map(|r| source.document.pointer(&r.pointer).unwrap().clone())
            .collect::<Vec<_>>();
        let text = serde_json::to_string(&values)
            .map_err(|_| "PREPARATION_RENDER stage=whole_preparation")?;
        *result
            .pointer_mut(&source.target)
            .ok_or("PREPARATION_RENDER_TARGET stage=whole_preparation")? = Value::String(text);
    }
    Ok(result)
}
fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
