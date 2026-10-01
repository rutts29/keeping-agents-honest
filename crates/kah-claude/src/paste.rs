//! Claude 2.1.287's native pasted_content framing (four lowercase hex digits).
//! Decode only one complete native block; never search arbitrary quoted text.
pub fn body(text: &str) -> Option<&str> {
    let text = text.trim_matches('\n');
    let (id, rest) = header(text)?;
    rest.strip_suffix(&format!("\n</pasted_content id=\"{id}\">"))
}
/// Classification is conservative even when someone appends bytes to a relay
/// envelope. Such input cannot become human authority by breaking its framing.
pub fn content_start(text: &str) -> &str {
    header(text.trim_start_matches('\n')).map_or(text, |(_, body)| body)
}
fn header(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix("<pasted_content id=\"")?;
    let id = text.get(..4)?;
    if !id
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some((id, text.get(4..)?.strip_prefix("\">\n")?))
}
