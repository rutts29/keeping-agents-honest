use crate::Result;
use kah_codex::{
    control::Control,
    delivery::{self, Message},
};
use kah_core::run_store::RunStore;
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub fn run(args: &[String]) -> Result<()> {
    if args.len() < 3 || !(args.len() - 3).is_multiple_of(2) {
        return Err(
            "SEND_ARGUMENTS stage=arguments: kah send SESSION TEXT [--sender NAME] [--receipt ID]"
                .into(),
        );
    }
    let mut sender = "kah-builder";
    let mut receipt = None;
    for pair in args[3..].chunks_exact(2) {
        match pair[0].as_str() {
            "--sender" => sender = &pair[1],
            "--receipt" => receipt = Some(pair[1].as_str()),
            _ => return Err("SEND_ARGUMENTS stage=arguments".into()),
        }
    }
    deliver(&args[1], &args[2], sender, receipt, false)
}

pub fn user(session: &str, text: &str, receipt: Option<&str>) -> Result<()> {
    deliver(session, text, "operator", receipt, true)
}
pub fn linked_user(
    session: &str,
    text: &str,
    receipt: Option<&str>,
    supersedes: &str,
) -> Result<()> {
    let generated = format!(
        "r{:x}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    );
    println!(
        "{}",
        crate::user_steer::deliver_linked(
            session,
            text,
            receipt.unwrap_or(&generated),
            Some(supersedes)
        )?
    );
    Ok(())
}
fn deliver(
    session: &str,
    text: &str,
    sender: &str,
    receipt: Option<&str>,
    user: bool,
) -> Result<()> {
    let generated = format!(
        "r{:x}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    );
    let receipt = receipt.unwrap_or(&generated);
    let message = if user {
        Message::user(session, receipt, text)?
    } else {
        Message::new(session, sender, receipt, text)?
    };
    eprintln!("kah message receipt: {receipt} (reuse --receipt on retry)");
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("SEND_HOME stage=arguments")?);
    let base = RunStore::default_state_root()?;
    let overlay = crate::session_policy::saved(&base, session)?
        .map(|p| p.overlay.join("app-server-control/app-server-control.sock"));
    let endpoint = crate::relay_store::endpoint(session).ok();
    let embedded = endpoint
        .as_ref()
        .is_some_and(|e| e.provider == "claude" || overlay.is_none());
    let socket =
        overlay.unwrap_or_else(|| home.join(".codex/app-server-control/app-server-control.sock"));
    if user && (!socket.exists() || embedded) {
        println!("{}", crate::user_steer::deliver(session, text, receipt)?);
        return Ok(());
    }
    let mut client = match Control::connect_daemon(&socket, Duration::from_secs(20)) {
        Ok(client) => client,
        Err(_) if user => {
            println!("{}", crate::user_steer::deliver(session, text, receipt)?);
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let result = delivery::send(&RunStore::default_state_root()?, &message, &mut client)?;
    println!("{result}");
    Ok(())
}
