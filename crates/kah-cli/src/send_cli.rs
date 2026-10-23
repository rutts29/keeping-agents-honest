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
mod herdr;
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

/// The message an `kah send` delegate delivers. Herdr types only its text, so
/// the sender prefix alone must mark it as delegate context in every adapter.
pub fn delegate_message(session: &str, sender: &str, receipt: &str, text: &str) -> Result<Message> {
    if sender == "kah-user" {
        return Err("SEND_ARGUMENTS stage=delegate_channel: reserved user sender".into());
    }
    if !kah_core::intent_history::is_kah_delegate_message(&format!("[{sender}]")) {
        return Err("SEND_SENDER stage=delegate_channel: a sender is kah- followed by lowercase letters, digits or -".into());
    }
    Ok(Message::new(session, sender, receipt, text)?)
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
        delegate_message(session, sender, receipt, text)?
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
    if (user || endpoint.is_some()) && (!socket.exists() || embedded) {
        println!("{}", fallback(&message, text, receipt, user)?);
        return Ok(());
    }
    let mut client = match Control::connect_daemon(&socket, Duration::from_secs(20)) {
        Ok(client) => client,
        Err(_) if user || endpoint.is_some() => {
            println!("{}", fallback(&message, text, receipt, user)?);
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    // An overlay socket may point to a daemon that does not own this --no-daemon
    // TUI. Check native in-memory ownership BEFORE any receipt/payload mutation.
    if (user || endpoint.is_some())
        && !delivery::owns_session(&mut client, session).unwrap_or(false)
    {
        println!("{}", fallback(&message, text, receipt, user)?);
        return Ok(());
    }
    let result = delivery::send(&RunStore::default_state_root()?, &message, &mut client)?;
    println!("{result}");
    Ok(())
}
fn fallback(message: &Message, text: &str, receipt: &str, user: bool) -> Result<serde_json::Value> {
    if user {
        crate::user_steer::deliver(&message.session, text, receipt)
    } else {
        herdr::deliver(message)
    }
}
