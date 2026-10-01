//! Scrub credential fields and common credential syntax before sending or saving.
use serde_json::Value;
const FIELDS: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "password",
    "access_token",
    "refresh_token",
    "client_secret",
    "authorization",
    "private_key",
    "secret_access_key",
];

pub(super) fn scrub(value: &mut Value) -> usize {
    match value {
        Value::Object(map) => map
            .iter_mut()
            .map(|(key, value)| {
                let key = key.to_ascii_lowercase();
                if value.is_string()
                    && FIELDS
                        .iter()
                        .any(|field| key == *field || key.ends_with(&format!("_{field}")))
                {
                    *value = "[REDACTED]".into();
                    1
                } else {
                    scrub(value)
                }
            })
            .sum(),
        Value::Array(values) => values.iter_mut().map(scrub).sum(),
        Value::String(text) => {
            if let Ok(mut nested) = serde_json::from_str::<Value>(text)
                && (nested.is_object() || nested.is_array())
            {
                let count = scrub(&mut nested);
                if count > 0 {
                    *text = nested.to_string();
                    return count;
                }
            }
            let (clean, count) = scrub_text(text);
            *text = clean;
            count
        }
        _ => 0,
    }
}
fn scrub_text(text: &str) -> (String, usize) {
    let mut output = String::new();
    let mut count = 0;
    let mut private_key = false;
    for line in text.split_inclusive('\n') {
        if private_key || line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----") {
            if !private_key {
                output.push_str("[REDACTED PRIVATE KEY]\n");
                count += 1;
            }
            private_key = !line.contains("-----END ");
            continue;
        }
        let line = assignments(line, &mut count);
        for piece in line.split_inclusive(|c: char| !token_char(c)) {
            let token = piece.trim_end_matches(|c: char| !token_char(c));
            if secret_token(token) {
                output.push_str("[REDACTED]");
                output.push_str(&piece[token.len()..]);
                count += 1;
            } else {
                output.push_str(piece);
            }
        }
    }
    (output, count)
}
fn assignment(line: &str) -> Option<usize> {
    let lower = line.to_ascii_lowercase();
    let mut first = None;
    for field in FIELDS {
        for (at, _) in lower.match_indices(field) {
            let after = &line[at + field.len()..];
            let tail = after.trim_start_matches([' ', '\t', '"', '\'', '\\']);
            if tail.starts_with([':', '=']) {
                let value = tail[1..].trim_start_matches([' ', '\t']);
                let start = line.len() - value.len();
                if !value.trim().is_empty() {
                    first = Some(first.map_or(start, |old: usize| old.min(start)));
                }
            }
        }
    }
    first
}

fn assignments(mut text: &str, count: &mut usize) -> String {
    let mut output = String::new();
    while let Some(start) = assignment(text) {
        let end = start + value_length(&text[start..]);
        if end == start {
            break;
        }
        output.push_str(&text[..start]);
        output.push_str("[REDACTED]");
        *count += 1;
        text = &text[end..];
    }
    output.push_str(text);
    output
}

fn value_length(text: &str) -> usize {
    if text.starts_with("\\\"") || text.starts_with("\\'") {
        return text[2..].find(&text[..2]).map_or(text.len(), |end| end + 4);
    }
    let first = text.chars().next().unwrap_or(' ');
    if matches!(first, '\'' | '"') {
        let mut escaped = false;
        for (at, c) in text.char_indices().skip(1) {
            if c == first && !escaped {
                return at + 1;
            }
            escaped = c == '\\' && !escaped;
        }
        return text.len();
    }
    let scheme = if text.to_ascii_lowercase().starts_with("bearer ") {
        7
    } else if text.to_ascii_lowercase().starts_with("basic ") {
        6
    } else {
        0
    };
    scheme
        + text[scheme..]
            .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '}' | ')'))
            .unwrap_or(text.len() - scheme)
}
fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}
fn secret_token(token: &str) -> bool {
    token.len() >= 24
        && ["sk-", "ghp_", "github_pat_", "AIza", "xoxb-", "xoxp-"]
            .iter()
            .any(|prefix| token.starts_with(prefix))
        || token.len() > 30 && token.starts_with("eyJ") && token.matches('.').count() == 2
}
