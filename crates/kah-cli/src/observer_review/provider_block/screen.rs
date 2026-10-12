//! Codex rust-v0.162.1 tui/src/chatwidget/misalignment_policy.rs:125-183.
//! Match the bottom selection view, never a discussion of the native message.
pub fn active(screen: &str) -> bool {
    let normalized = screen.to_lowercase();
    let lines = normalized.lines().map(str::trim).collect::<Vec<_>>();
    let Some(title) = lines.iter().rposition(|s| {
        matches!(
            *s,
            "chat paused as a precaution" | "chat stopped as a precaution"
        )
    }) else {
        return false;
    };
    let tail = &lines[title + 1..];
    let Some(footer) = tail
        .iter()
        .rposition(|s| s.contains("enter to select") || s.contains("enter to confirm"))
    else {
        return false;
    };
    if tail[footer + 1..]
        .iter()
        .any(|s| s.starts_with('›') || s.starts_with('❯'))
    {
        return false;
    }
    let options = tail[..footer]
        .iter()
        .filter_map(|s| option(s))
        .collect::<Vec<_>>();
    (options.contains(&"new chat") && options.contains(&"resume another chat"))
        || options.contains(&"acknowledge findings and continue")
}
fn option(line: &str) -> Option<&str> {
    let line = line.trim_start_matches(['›', '❯']).trim();
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0)
        .then(|| line.get(digits..)?.strip_prefix(". "))
        .flatten()
}
