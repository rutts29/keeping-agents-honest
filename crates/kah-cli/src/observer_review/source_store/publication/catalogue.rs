use super::*;
pub(super) fn files(private: &Path, input: &Path, catalogue: &Value) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for key in ["full_request", "intent", "delta", "schema", "record_index"] {
        if let Some(path) = catalogue[key].as_str()
            && Path::new(path).is_file()
        {
            files.insert(PathBuf::from(path));
        }
    }
    for source in catalogue["sources"]
        .as_array()
        .ok_or("REVIEW_CONTEXT_CATALOGUE stage=source_view")?
    {
        let path = PathBuf::from(
            source["path"]
                .as_str()
                .ok_or("REVIEW_SOURCE_PATH stage=source_view")?,
        );
        if matches!(
            source["kind"].as_str(),
            Some("lossless_reference" | "lossless_tree_catalog")
        ) {
            let raw: Value = serde_json::from_slice(&fs::read(&path)?)?;
            let text = super::super::expand(raw, &private.join("sources"))?;
            let index: Value = serde_json::from_str(
                text.as_str()
                    .ok_or("REVIEW_SOURCE_ENCODING stage=source_view")?,
            )?;
            if let Some(external) = index["full_source"].as_str() {
                files.insert(PathBuf::from(external));
            }
        }
        files.insert(path);
    }
    // Preparation manifests are complete decisions belonging to this request.
    for entry in fs::read_dir(input)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("preparation-")
        {
            files.insert(entry.path());
        }
    }
    Ok(files)
}
