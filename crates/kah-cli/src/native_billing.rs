//! Guard the native bottom-pane purchase/usage picker, not transcript prose.
//! Detects the native purchase/usage picker on screen, checked against Codex
//! 0.160.0 and Claude Code 2.1.292. No native billing action is invoked.
pub fn active(screen: &str) -> bool {
    let lines: Vec<String> = screen
        .lines()
        .map(|line| line.trim().to_lowercase())
        .collect();
    if amount_dialog(&lines) {
        return true;
    }
    let Some(footer) = lines.iter().rposition(|line| {
        line.contains("press a number to choose")
            || ((line.contains("to select") || line.contains("to confirm")) && line.contains("esc"))
    }) else {
        return false;
    };
    // A newer composer/answer means this picker is transcript history. Codex's
    // inline actionable banner deliberately retains its empty composer below it.
    if lines[footer + 1..].iter().any(|line| {
        composer(line).is_some_and(|text| !text.is_empty() && text != "ask codex to do anything")
    }) {
        return false;
    }
    let Some(last) = lines[..footer].iter().rposition(|line| option(line)) else {
        return false;
    };
    // Walk the actual option group backwards; no fixed screen window and no
    // older picker elsewhere in the transcript can contribute its CTA text.
    let first = lines[..=last]
        .iter()
        .rposition(|line| !line.is_empty() && !option(line))
        .map_or(0, |index| index + 1);
    let group = &lines[first..=last];
    let usage = lines[..first]
        .iter()
        .rev()
        .take_while(|line| composer(line).is_none())
        .any(|line| line == "usage limit reached");
    usage || group.iter().any(|line| billing_option(line))
}
fn composer(line: &str) -> Option<&str> {
    line.strip_prefix('›')
        .or_else(|| line.strip_prefix('❯'))
        .map(str::trim)
}
fn option(line: &str) -> bool {
    let text = composer(line).unwrap_or(line);
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && text
            .get(digits..)
            .is_some_and(|tail| tail.starts_with(". "))
}

fn billing_option(line: &str) -> bool {
    [
        "billing", "credit", "purchase", "upgrade", "payment", "usage", "reset",
    ]
    .iter()
    .any(|word| line.contains(word))
}

// Bordered amount input with a native confirmation line or card-charge
// disclosure; these are inputs, not pickers.
fn amount_dialog(lines: &[String]) -> bool {
    let Some(start) = lines.iter().rposition(|line| {
        line == "buy usage credits" || line == "confirm amount" || line == "set monthly spend limit"
    }) else {
        return false;
    };
    let block = &lines[start + 1..];
    !block.iter().any(|line| composer(line).is_some())
        && block.iter().any(|line| line.contains('│'))
        && block.iter().any(|line| {
            line == "type yes to confirm."
                || line
                    == "by confirming, you allow anthropic to charge your card in the amount above."
        })
}
