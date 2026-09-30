//! Read-only native provenance audit; prints record hashes and classes, never text.
use kah_codex::{control::Control, source};
use std::{env, path::PathBuf, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME absent")?);
    let session = env::args().nth(1).ok_or("session ID required")?;
    let mut client = Control::connect_daemon(
        &home.join(".codex/app-server-control/app-server-control.sock"),
        Duration::from_secs(20),
    )?;
    println!("{}", source::provenance_audit(&mut client, &session)?);
    Ok(())
}
