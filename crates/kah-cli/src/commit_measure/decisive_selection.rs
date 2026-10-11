//! Brain-selected whole evidence units; source and exclusions remain auditable.
use super::*;

pub fn apply(original: &Value, manifest: &Value) -> Result<Value> {
    let hunks = original["state"]["diff_hunks"]
        .as_object()
        .ok_or("MEASURE_SELECTION_HUNKS stage=selection")?;
    let selections = manifest["hunks"]
        .as_array()
        .ok_or("MEASURE_SELECTION_MANIFEST stage=selection")?;
    let mut result = original.clone();
    let mut seen = std::collections::BTreeSet::new();
    for selection in selections {
        let id = selection["id"]
            .as_str()
            .ok_or("MEASURE_SELECTION_ID stage=selection")?;
        let source = hunks
            .get(id)
            .and_then(|h| h["text"].as_str())
            .ok_or("MEASURE_SELECTION_SOURCE stage=selection")?;
        if !seen.insert(id) || selection["source_digest"] != commit_check::digest(source.as_bytes())
        {
            return Err("MEASURE_SELECTION_DIGEST stage=selection".into());
        }
        result["state"]["diff_hunks"][id]["text"] = json!(select(source, selection)?);
    }
    if seen.len() != hunks.len() {
        return Err("MEASURE_SELECTION_MISSING stage=selection".into());
    }
    Ok(result)
}
fn select(source: &str, manifest: &Value) -> Result<String> {
    let lines: Vec<_> = source.split_inclusive('\n').collect();
    let mut covered = vec![false; lines.len()];
    let mut selected = BTreeMap::new();
    for category in ["selected", "excluded"] {
        for span in manifest[category]
            .as_array()
            .ok_or("MEASURE_SELECTION_SPANS stage=selection")?
        {
            let (first, last) = range(span, lines.len())?;
            if span["reason"].as_str().is_none_or(|r| r.trim().is_empty()) {
                return Err("MEASURE_SELECTION_REASON stage=selection".into());
            }
            for seen in &mut covered[first..last] {
                if *seen {
                    return Err("MEASURE_SELECTION_OVERLAP stage=selection".into());
                }
                *seen = true;
            }
            if category == "selected" {
                let text = lines[first..last].concat();
                if span["text"] != text || span["unit"].as_str().is_none_or(|s| s.trim().is_empty())
                {
                    return Err("MEASURE_SELECTION_QUOTE stage=selection".into());
                }
                selected.insert(first, text);
            }
        }
    }
    if covered.contains(&false) || selected.is_empty() {
        return Err("MEASURE_SELECTION_ACCOUNTING stage=selection".into());
    }
    Ok(selected.into_values().collect::<Vec<_>>().join("\n\n"))
}
fn range(span: &Value, length: usize) -> Result<(usize, usize)> {
    let first = span["first_line"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("MEASURE_SELECTION_RANGE stage=selection")?;
    let last = span["last_line"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("MEASURE_SELECTION_RANGE stage=selection")?;
    if first == 0 || last < first || last > length {
        return Err("MEASURE_SELECTION_RANGE stage=selection".into());
    }
    Ok((first - 1, last))
}
