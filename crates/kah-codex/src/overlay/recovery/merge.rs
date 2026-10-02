use serde_json::Value;
use std::collections::BTreeMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

fn rows(bytes: &[u8]) -> Result<Vec<Value>, String> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err("partial JSONL record; writer must finish first".into());
    }
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_slice(line)
                .map_err(|_| "invalid JSONL record (content withheld)".into())
        })
        .collect()
}
fn string<'a>(row: &'a Value, key: &str) -> Result<&'a str, String> {
    row[key]
        .as_str()
        .ok_or_else(|| format!("missing/invalid {key} field (content withheld)"))
}
fn date(row: &Value, key: &str) -> Result<OffsetDateTime, String> {
    OffsetDateTime::parse(string(row, key)?, &Rfc3339)
        .map_err(|_| format!("invalid {key} timestamp (content withheld)"))
}
fn line(row: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(row).unwrap();
    bytes.push(b'\n');
    bytes
}
pub(super) fn index(global: &[u8], local: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut newest = BTreeMap::new();
    let mut last_global = BTreeMap::new();
    for (is_global, rows) in [(true, rows(global)?), (false, rows(local)?)] {
        for row in rows {
            let id = string(&row, "id")?.to_owned();
            string(&row, "thread_name")?;
            let updated = date(&row, "updated_at")?;
            if is_global {
                last_global.insert(id.clone(), row.clone());
            }
            if let Some((old_date, old_row)) = newest.get(&id) {
                if updated == *old_date && row != *old_row {
                    return Err("conflicting names at the same updated_at timestamp".into());
                }
                if updated <= *old_date {
                    continue;
                }
            }
            newest.insert(id, (updated, row));
        }
    }
    Ok(newest
        .into_iter()
        .filter(|(id, (_, row))| last_global.get(id) != Some(row))
        .map(|(_, (_, row))| line(&row))
        .collect())
}
pub(super) fn history(global: &[u8], local: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let global = rows(global)?;
    let local = rows(local)?;
    let mut counts = BTreeMap::new();
    let mut additions = vec![];
    for (is_local, rows) in [(false, global), (true, local)] {
        for row in rows {
            let id = string(&row, "session_id")?;
            let text = string(&row, "text")?;
            let ts = row["ts"]
                .as_u64()
                .ok_or("invalid history timestamp (content withheld)")?;
            let count = counts
                .entry((id.to_owned(), ts, text.to_owned()))
                .or_insert(0usize);
            if !is_local {
                *count += 1;
            } else if *count > 0 {
                *count -= 1;
            } else {
                additions.push(line(&row));
            }
        }
    }
    Ok(additions)
}
pub(super) fn auth(global: &[u8], local: &[u8]) -> Result<bool, String> {
    let parse = |bytes| {
        serde_json::from_slice::<Value>(bytes).map_err(|_| "invalid auth JSON (content withheld)")
    };
    let global = parse(global)?;
    let local = parse(local)?;
    if !global.is_object() || !local.is_object() {
        return Err("invalid auth object".into());
    }
    if global == local {
        return Ok(false);
    }
    let account = string(&global["tokens"], "account_id")?;
    if account.is_empty() || account != string(&local["tokens"], "account_id")? {
        return Err("different auth accounts; cannot safely choose a login".into());
    }
    for row in [&global, &local] {
        for key in ["access_token", "refresh_token", "id_token"] {
            if string(&row["tokens"], key)?.is_empty() {
                return Err("incomplete auth tokens (content withheld)".into());
            }
        }
        if row["auth_mode"] != global["auth_mode"] {
            return Err("different auth modes".into());
        }
    }
    let (g, l) = (
        date(&global, "last_refresh")?,
        date(&local, "last_refresh")?,
    );
    if g == l {
        return Err(
            "different auth material at the same last_refresh; cannot choose safely".into(),
        );
    }
    Ok(l > g)
}
