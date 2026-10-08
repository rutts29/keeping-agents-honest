//! Claude Code 2.1.292 project keys, matching shipped uI/k/Le/_re functions.
//! Native transcript path encoding follows the provider version named above.
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// Claude2.1.292 we(): explicit native root or HOME/.claude, NFC-normalized.
pub fn config_root(home: &Path) -> PathBuf {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    root.to_string_lossy().nfc().collect::<String>().into()
}
/// Native production Gbr(): explicit config dir also owns .claude.json.
pub fn global_config(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.to_path_buf())
        .join(".claude.json")
}

pub fn project_key(project: &Path) -> String {
    let text = project.to_string_lossy();
    let units = text.encode_utf16().collect::<Vec<_>>();
    let key = units
        .iter()
        .map(|u| {
            if *u <= 127 && (*u as u8).is_ascii_alphanumeric() {
                *u as u8 as char
            } else {
                '-'
            }
        })
        .collect::<String>();
    if key.len() <= 200 {
        return key;
    }
    let hash = units.iter().fold(0i32, |value, u| {
        value.wrapping_mul(31).wrapping_add(*u as i32)
    });
    format!("{}-{}", &key[..200], base36(hash.unsigned_abs()))
}
fn base36(mut value: u32) -> String {
    let mut chars = vec![];
    loop {
        chars.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % 36) as usize] as char);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    chars.into_iter().rev().collect()
}
pub fn transcript(home: &Path, project: &Path, session: &str) -> PathBuf {
    config_root(home)
        .join("projects")
        .join(project_key(project))
        .join(format!("{session}.jsonl"))
}
