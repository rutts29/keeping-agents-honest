use std::io::{self, Write};

pub fn send(output: &[u8], input: &str, writer: &mut impl Write) -> io::Result<()> {
    let mut terminal = vt100::Parser::new(36, 120, 0);
    terminal.process(output);
    // Check rendered cells for ANSI styling/redraws, plus raw history so a
    // cleared plain-text billing prompt still prevents further interaction.
    let text = format!(
        "{}\n{}",
        terminal.screen().contents(),
        String::from_utf8_lossy(output)
    )
    .to_ascii_lowercase();
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if ["billing", "credit", "purchase", "upgrade", "payment"]
        .iter()
        .any(|word| compact.contains(word))
    {
        return Err(io::Error::other(
            "FIXTURE_BILLING_PROMPT stage=input: abort",
        ));
    }
    writer.write_all(input.as_bytes())?;
    writer.flush()
}
