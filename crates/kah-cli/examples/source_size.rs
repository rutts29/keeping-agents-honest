//! Read-only source sizing; prints counts, never transcript content.
use kah_codex::{control::Control, source};
use std::{env, path::PathBuf, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME absent")?);
    let session = env::args().nth(1).ok_or("session ID required")?;
    let root = env::current_dir()?;
    let mut client = Control::connect_daemon(
        &home.join(".codex/app-server-control/app-server-control.sock"),
        Duration::from_secs(20),
    )?;
    let snapshot = source::observe(&mut client, &root, &session)?;
    if let Some(users) = snapshot.users {
        println!(
            "{}",
            serde_json::json!({
                "userRecords":users.authoritative_records().len(),
                "userBytes":users.authoritative_records().iter().map(|r|r.content().len()).sum::<usize>(),
                "boundedActions":snapshot.actions.len()
            })
        );
    }
    Ok(())
}
