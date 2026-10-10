//! Recover code from the original root-commit patch/native patch evidence.
//! Original source states remain retained alongside every derived file.
use super::*;
use kah_observer::commit_check::CodeFile;
pub(super) fn coverage(state: &Value, revision: &str) -> Result<BTreeMap<String, CodeFile>> {
    let requirements = state["decision"]["requirements"]
        .as_object()
        .ok_or("MEASURE_COVERAGE_STATE stage=source")?;
    let requirement = requirements
        .values()
        .next()
        .ok_or("MEASURE_COVERAGE_STATE stage=source")?;
    let mut files = BTreeMap::new();
    for citation in requirement["citations"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
    {
        if citation["kind"] != "committed_code" {
            continue;
        }
        if let Some(patch) = citation["quote"].as_str() {
            files.extend(root_files(patch, revision));
        }
    }
    if let Some(command) = state["event"]["selected_host_check"]["command"].as_str() {
        files.insert(
            "selected-check-command.sh".into(),
            CodeFile::new(revision, command),
        );
    }
    if files.is_empty() {
        return Err("MEASURE_CODE_SOURCE_MISSING stage=source: coverage quotes contain no complete root file or selected command".into());
    }
    Ok(files)
}
fn root_files(patch: &str, revision: &str) -> BTreeMap<String, CodeFile> {
    let mut result = BTreeMap::new();
    let mut name = None;
    let mut complete = false;
    let mut code = String::new();
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            finish(&mut result, name.take(), complete, &code, revision);
            complete = false;
            code.clear();
        } else if line.starts_with("new file mode ") {
            complete = true;
        } else if let Some(path) = line.strip_prefix("+++ b/") {
            name = Some(path.trim_end_matches('\n').to_owned());
        } else if let Some(text) = line.strip_prefix('+') {
            code.push_str(text);
        }
    }
    finish(&mut result, name, complete, &code, revision);
    result
}
fn finish(
    files: &mut BTreeMap<String, CodeFile>,
    name: Option<String>,
    complete: bool,
    text: &str,
    revision: &str,
) {
    if complete && let Some(name) = name {
        files.insert(name, CodeFile::new(revision, text));
    }
}
pub(super) fn drift(original: &Value, revision: &str) -> Result<BTreeMap<String, CodeFile>> {
    for finding in original["fullVerifier"]["report"]["findings"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if finding["topic"] != "drift" {
            continue;
        }
        for citation in finding["citations"].as_array().into_iter().flatten() {
            let Some(quote) = citation["quote"].as_str() else {
                continue;
            };
            if quote.starts_with("module.exports") {
                return Ok(BTreeMap::from([(
                    "double.cjs".into(),
                    CodeFile::new(revision, quote),
                )]));
            }
            for line in quote.split_inclusive('\n') {
                if let Some(code) = line.strip_prefix('+')
                    && code.starts_with("module.exports")
                {
                    return Ok(BTreeMap::from([(
                        "double.cjs".into(),
                        CodeFile::new(revision, code),
                    )]));
                }
            }
        }
    }
    Err("MEASURE_CODE_SOURCE_MISSING stage=source: planted native patch code absent".into())
}
