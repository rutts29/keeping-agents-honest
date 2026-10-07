//! git/git v2.53.0 Documentation/git-cat-file.adoc BATCH OUTPUT.
//! One native batch reads raw blobs, without filters or symlink following.
use crate::Result;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Cursor, Read},
    path::Path,
};
pub fn collect(root: &Path, head: &str) -> Result<Vec<Value>> {
    let raw =
        super::super::artifact_git::git(root, &["ls-tree", "-rz", "--full-tree", head], None)?;
    let entries = entries(&raw)?;
    let input = entries
        .iter()
        .filter(|e| e.1 == "blob")
        .map(|e| format!("{}\n", e.2))
        .collect::<String>();
    let raw =
        super::super::artifact_git::git(root, &["cat-file", "--batch"], Some(input.into_bytes()))?;
    let mut output = Cursor::new(raw);
    let mut files = vec![];
    for (mode, kind, blob, path) in entries {
        if kind != "blob" {
            files.push(json!({"path":path,"mode":mode,"kind":kind,"object":blob}));
            continue;
        }
        let bytes = blob_content(&mut output, &blob)?;
        let content = match String::from_utf8(bytes) {
            Ok(text) => json!({"encoding":"utf8","text":text}),
            Err(error) => json!({"encoding":"bytes","bytes":error.into_bytes()}),
        };
        files.push(json!({"path":path,"mode":mode,"kind":kind,"blob":blob,"content":content}));
    }
    if output.position() != output.get_ref().len() as u64 {
        return Err("REVIEW_COMMIT_BATCH_TRAILING stage=context".into());
    }
    Ok(files)
}
fn entries(raw: &[u8]) -> Result<Vec<(String, String, String, String)>> {
    raw.split(|b| *b == 0)
        .filter(|e| !e.is_empty())
        .map(|entry| {
            let entry = std::str::from_utf8(entry)
                .map_err(|_| "REVIEW_COMMIT_PATH_ENCODING stage=context")?;
            let (header, path) = entry
                .split_once('\t')
                .ok_or("REVIEW_COMMIT_TREE stage=context")?;
            let fields = header.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err("REVIEW_COMMIT_TREE stage=context".into());
            }
            Ok((
                fields[0].into(),
                fields[1].into(),
                fields[2].into(),
                path.into(),
            ))
        })
        .collect()
}
fn blob_content(output: &mut Cursor<Vec<u8>>, expected: &str) -> Result<Vec<u8>> {
    let mut header = String::new();
    output.read_line(&mut header)?;
    let fields = header.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 || fields[0] != expected || fields[1] != "blob" {
        return Err("REVIEW_COMMIT_BATCH_HEADER stage=context".into());
    }
    let size = fields[2].parse::<u64>()?;
    let mut contents = Vec::new();
    output.take(size).read_to_end(&mut contents)?;
    if contents.len() as u64 != size {
        return Err("REVIEW_COMMIT_BATCH_CONTENT stage=context".into());
    }
    let mut terminator = [0];
    output.read_exact(&mut terminator)?;
    if terminator != *b"\n" {
        return Err("REVIEW_COMMIT_BATCH_TERMINATOR stage=context".into());
    }
    Ok(contents)
}
