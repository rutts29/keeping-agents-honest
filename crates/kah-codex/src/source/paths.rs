use super::*;

pub(super) fn provider_item_references_checkout(value: &Value, repository: &Path) -> bool {
    match value {
        Value::Object(fields) => fields
            .iter()
            .any(|(field, value)| field != "cwd" && value_references_checkout(value, repository)),
        _ => value_references_checkout(value, repository),
    }
}

pub(super) fn value_references_checkout(value: &Value, repository: &Path) -> bool {
    match value {
        Value::String(text) => text_references_checkout(text, repository),
        Value::Array(values) => values
            .iter()
            .any(|value| value_references_checkout(value, repository)),
        Value::Object(fields) => fields
            .values()
            .any(|value| value_references_checkout(value, repository)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

pub(super) fn text_references_checkout(text: &str, repository: &Path) -> bool {
    absolute_path_candidates(text)
        .iter()
        .any(|candidate| path_references_checkout(candidate, repository))
}

pub(super) fn absolute_path_candidates(text: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut search_from = 0;
    while let Some(relative_start) = text[search_from..].find('/') {
        let start = search_from + relative_start;
        let quote = text[..start]
            .chars()
            .next_back()
            .filter(|character| matches!(character, '\'' | '"'));
        let mut candidate = String::new();
        let mut escaped = false;
        let mut end = start;
        for (offset, character) in text[start..].char_indices() {
            let index_after = start + offset + character.len_utf8();
            if escaped {
                candidate.push(character);
                escaped = false;
                end = index_after;
                continue;
            }
            if character == '\\' && quote != Some('\'') {
                escaped = true;
                end = index_after;
                continue;
            }
            if quote.is_some_and(|delimiter| delimiter == character) {
                end = index_after;
                break;
            }
            if quote.is_none() && path_delimiter(character) {
                end = index_after;
                break;
            }
            candidate.push(character);
            end = index_after;
            if candidate.len() >= 16 * 1024 {
                break;
            }
        }
        if !candidate.is_empty() {
            candidates.push(PathBuf::from(&candidate));
            let without_prose_punctuation = candidate.trim_end_matches(['.', '?', '!']);
            if without_prose_punctuation.len() != candidate.len() {
                candidates.push(PathBuf::from(without_prose_punctuation));
            }
        }
        search_from = end.max(start + 1);
    }
    candidates
}

pub(super) fn path_references_checkout(candidate: &Path, repository: &Path) -> bool {
    if !candidate.is_absolute() {
        return false;
    }
    let normalized = lexically_normalize(candidate);
    path_has_component_prefix(candidate, repository)
        || path_has_component_prefix(&normalized, repository)
        || canonicalize_existing_ancestor(candidate)
            .is_some_and(|resolved| path_has_component_prefix(&resolved, repository))
        || canonicalize_existing_ancestor(&normalized)
            .is_some_and(|resolved| path_has_component_prefix(&resolved, repository))
}

pub(super) fn canonicalize_existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut ancestor = path.to_path_buf();
    let mut missing = Vec::<OsString>::new();
    loop {
        if let Ok(mut resolved) = fs::canonicalize(&ancestor) {
            for component in missing.iter().rev() {
                resolved.push(component);
            }
            return Some(lexically_normalize(&resolved));
        }
        let name = ancestor.file_name()?.to_os_string();
        missing.push(name);
        ancestor = ancestor.parent()?.to_path_buf();
    }
}

pub(super) fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

pub(super) fn path_has_component_prefix(path: &Path, repository: &Path) -> bool {
    let mut path_components = path.components();
    repository.components().all(|repository_component| {
        path_components.next().is_some_and(|path_component| {
            path_component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&repository_component.as_os_str().to_string_lossy())
        })
    })
}

fn path_delimiter(character: char) -> bool {
    character.is_whitespace() || "'\"`:,;|&<>()[]{}".contains(character)
}
