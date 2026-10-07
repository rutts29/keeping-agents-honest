use std::io::{self, Write};

pub fn send(output: &[u8], input: &str, writer: &mut impl Write) -> io::Result<()> {
    let mut terminal = vt100::Parser::new(36, 120, 0);
    terminal.process(output);
    if kah_cli::native_billing::active(&terminal.screen().contents()) {
        return Err(io::Error::other(
            "FIXTURE_BILLING_PROMPT stage=input: abort",
        ));
    }
    writer.write_all(input.as_bytes())?;
    writer.flush()
}
