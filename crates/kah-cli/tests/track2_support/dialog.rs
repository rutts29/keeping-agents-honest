use std::path::Path;
pub fn folder_dialog(provider: &str, screen: &str, expected: &Path) -> Result<bool, String> {
    let known = screen.contains("Trust this folder?") && screen.contains("Trust and continue");
    if known {
        if provider != "codex" {
            return Err("TRACK2_OTHER_DIALOG stage=trust".into());
        }
        let mut lines = screen.lines().skip_while(|s| s.trim() != "Folder access");
        lines.next();
        let shown = lines.find(|s| s.trim().starts_with('/')).map(str::trim);
        if shown != expected.to_str() || !screen.contains("› 1. Trust and continue") {
            return Err("TRACK2_TRUST_PATH stage=trust".into());
        }
        return Ok(true);
    }
    Err("TRACK2_OTHER_DIALOG stage=trust: permission, sandbox, login, update or unrecognized dialog; no keys sent".into())
}
/// A prompt is pending while its last appearance in the cumulative terminal log
/// comes after the last appearance of `settled`, the text the TUI redraws once
/// the prompt is answered. The log keeps old frames, so presence alone is not
/// proof that the prompt is still on screen.
pub fn prompt_pending(log: &str, prompt: &str, settled: &str) -> bool {
    match (log.rfind(prompt), log.rfind(settled)) {
        (Some(_), None) => true,
        (Some(shown), Some(redrawn)) => shown > redrawn,
        (None, _) => false,
    }
}
