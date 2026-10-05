//! Complete path inventory; unsupported paths are recorded, never silently lost.
use super::artifact_git::git;
use crate::Result;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

pub fn baseline(root: &Path) -> Result<Value> {
    if git(root, &["rev-parse", "--is-inside-work-tree"], None).is_err() {
        return Ok(
            json!({"blobs":{},"unsupported":{"repository":"no Git inventory exists before work"},"captured_ms":kah_observer::session::now_ms()}),
        );
    }
    let raw = git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        None,
    )?;
    let mut blobs = BTreeMap::new();
    let mut unsupported = BTreeMap::new();
    let mut regular = vec![];
    for bytes in raw.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = std::str::from_utf8(bytes)
            .map_err(|_| "REVIEW_ARTIFACT_PATH_UTF8 stage=artifact_baseline")?;
        let full = root.join(path);
        let meta = match fs::symlink_metadata(&full) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                unsupported.insert(path.to_owned(), "absent before work");
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if meta.is_symlink() {
            use std::os::unix::ffi::OsStrExt;
            let target = fs::read_link(&full)?;
            let hash = git(
                root,
                &["hash-object", "--stdin"],
                Some(target.as_os_str().as_bytes().to_vec()),
            )?;
            blobs.insert(path.to_owned(), String::from_utf8(hash)?.trim().to_owned());
        } else if meta.is_file() && full.canonicalize()?.starts_with(root.canonicalize()?) {
            regular.push(path.to_owned());
        } else {
            unsupported.insert(path.to_owned(), "not a regular in-project file");
        }
    }
    hash_regular(root, regular, &mut blobs)?;
    Ok(
        json!({"blobs":blobs,"unsupported":unsupported,"captured_ms":kah_observer::session::now_ms()}),
    )
}
fn hash_regular(
    root: &Path,
    regular: Vec<String>,
    blobs: &mut BTreeMap<String, String>,
) -> Result<()> {
    if !regular.is_empty() {
        let input = regular
            .iter()
            .map(|p| super::artifact_git::quote(p) + "\n")
            .collect::<String>();
        let hashes = String::from_utf8(git(
            root,
            &["hash-object", "--no-filters", "--stdin-paths"],
            Some(input.into_bytes()),
        )?)?;
        if hashes.lines().count() != regular.len() {
            return Err("REVIEW_ARTIFACT_COUNT stage=artifact_baseline".into());
        }
        blobs.extend(regular.into_iter().zip(hashes.lines().map(str::to_owned)));
    }
    Ok(())
}
pub fn committed(root: &Path, head: &str) -> Result<BTreeMap<String, String>> {
    let raw = git(root, &["ls-tree", "-rz", "--full-tree", head], None)?;
    let mut blobs = BTreeMap::new();
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry)
            .map_err(|_| "REVIEW_ARTIFACT_PATH_UTF8 stage=artifact_committed")?;
        let (header, path) = entry
            .split_once('\t')
            .ok_or("REVIEW_ARTIFACT_TREE stage=artifact_committed")?;
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err("REVIEW_ARTIFACT_TREE stage=artifact_committed".into());
        }
        if fields[1] == "blob" {
            blobs.insert(path.into(), fields[2].into());
        }
    }
    Ok(blobs)
}
pub fn cli() -> Result<()> {
    println!("{}", baseline(&std::env::current_dir()?)?);
    Ok(())
}
