//! #52b: which pages of which source an observer was served, per review request, and the submit check that
//! every page of the request's required evidence (context-input.json `sources`) was read.
//! The ledger is `<workspace>/outbox/read-pages-<request view>.jsonl`, one line per served page, keyed by the
//! digest of the expanded source text, so a changed source is a different source. The outbox is the one place
//! the observer's sandbox may write; this guards against an unread page, not against a forged ledger.
use super::paging;
use crate::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// Pages of one source that were not read.
pub struct Unread {
    path: PathBuf,
    total: usize,
    pages: Vec<usize>,
}
fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
/// `<workspace>/inputs/<name>/...` -> (`<workspace>`, `<name>`), also for a file below `sources/`.
fn view(path: &Path) -> Option<(PathBuf, String)> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut directory = absolute.parent()?;
    loop {
        let parent = directory.parent()?;
        if parent.file_name().is_some_and(|n| n == "inputs") {
            return Some((
                parent.parent()?.to_owned(),
                directory.file_name()?.to_str()?.to_owned(),
            ));
        }
        directory = parent;
    }
}
fn ledger(workspace: &Path, name: &str) -> PathBuf {
    workspace
        .join("outbox")
        .join(format!("read-pages-{name}.jsonl"))
}
/// Notes that `page` of `pages` of the source `text` (read as `named`) was served. Outside a request view
/// there is nothing to note.
pub fn record(named: &Path, text: &str, page: usize, pages: usize) -> Result<()> {
    let Some((workspace, name)) = view(named) else {
        return Ok(());
    };
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o660)
        .open(ledger(&workspace, &name))?;
    let line = json!({"digest":digest(text),"bytes":text.len(),"page":page,"pages":pages,
        "path":named.display().to_string()});
    writeln!(file, "{line}")?;
    Ok(())
}
fn served(workspace: &Path, name: &str) -> BTreeMap<String, BTreeSet<usize>> {
    let mut served: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let text = fs::read_to_string(ledger(workspace, name)).unwrap_or_default();
    for line in text.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let (Some(digest), Some(page)) = (row["digest"].as_str(), row["page"].as_u64()) {
            served
                .entry(digest.to_owned())
                .or_default()
                .insert(page as usize);
        }
    }
    served
}
/// Pages of the request's listed sources that no `read-source` call served. A request without a source list
/// has nothing required; a source that cannot be expanded here cannot have been read either, so it is skipped.
pub fn unread(request: &Path) -> Result<Vec<Unread>> {
    let Some(directory) = request.parent() else {
        return Ok(vec![]);
    };
    let Some((workspace, name)) = view(request) else {
        return Ok(vec![]);
    };
    let Ok(catalogue) = fs::read(directory.join("context-input.json")) else {
        return Ok(vec![]);
    };
    let catalogue: Value = serde_json::from_slice(&catalogue)?;
    let served = served(&workspace, &name);
    let mut out = vec![];
    for source in catalogue["sources"].as_array().into_iter().flatten() {
        let Some(path) = source["path"].as_str().map(PathBuf::from) else {
            continue;
        };
        let Ok(text) = super::expanded_text(&path) else {
            continue;
        };
        let read = served.get(&digest(&text));
        let total = paging::count(&text);
        let pages: Vec<usize> = (1..=total)
            .filter(|p| !read.is_some_and(|r| r.contains(p)))
            .collect();
        if !pages.is_empty() {
            out.push(Unread { path, total, pages });
        }
    }
    Ok(out)
}
fn ranges(pages: &[usize]) -> String {
    let mut parts: Vec<(usize, usize)> = vec![];
    for &p in pages {
        match parts.last_mut() {
            Some((_, end)) if *end + 1 == p => *end = p,
            _ => parts.push((p, p)),
        }
    }
    let text = parts
        .iter()
        .map(|&(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}-{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("page{} {text}", if pages.len() == 1 { "" } else { "s" })
}
/// Pages listed with their commands per source; a long list shows the first ones, since each page's footer
/// names the command for the next.
const LISTED: usize = 12;
fn message(unread: &[Unread]) -> String {
    let mut out = String::from(
        "REVIEW_EVIDENCE_UNREAD stage=report: the report was not submitted because required evidence pages were not read. \
Read them with these commands, then submit again:",
    );
    for source in unread {
        let name = source
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        out.push_str(&format!(
            "\n{name}: {} of {}",
            ranges(&source.pages),
            source.total
        ));
        let path = source.path.display().to_string();
        for &page in source.pages.iter().take(LISTED) {
            out.push_str(&format!("\n  {}", paging::command(&path, page)));
        }
        if source.pages.len() > LISTED {
            out.push_str(&format!(
                "\n  ... {} more pages; each page ends with the command for the next",
                source.pages.len() - LISTED
            ));
        }
    }
    out
}
/// The submit check: Ok when every page of the request's required evidence was served.
pub fn require_read(request: &Path) -> Result<()> {
    let unread = unread(request)?;
    if unread.is_empty() {
        Ok(())
    } else {
        Err(message(&unread).into())
    }
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/source_read_ledger.rs"]
mod tests;
