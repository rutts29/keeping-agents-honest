//! Conservative replay-only guard: quarantine credential-shaped input, never redact into a new sample.
pub fn credential_shape(text: &str) -> bool {
    text.contains("-----BEGIN PRIVATE KEY-----")
        || text.contains("-----BEGIN OPENSSH PRIVATE KEY-----")
        || text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .any(|word| {
                (word.starts_with("sk-") && word.len() >= 27)
                    || (word.starts_with("ghp_") && word.len() >= 24)
                    || (word.starts_with("github_pat_") && word.len() >= 31)
                    || (word.starts_with("AIza") && word.len() >= 29)
            })
}
