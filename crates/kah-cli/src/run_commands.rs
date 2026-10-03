use crate::Result;
use kah_core::run_store::RunStore;
use kah_run::{commands, store::Record};

pub fn handles(args: &[String]) -> bool {
    matches!(
        args.first().map(String::as_str),
        Some("steer" | "stop" | "logs")
    ) || args.first().is_some_and(|s| s == "status")
        && args.get(1).is_some_and(|s| s.starts_with("run-"))
}
pub fn run(args: Vec<String>) -> Result<()> {
    let base = RunStore::default_state_root()?.join("runs");
    let id = args.get(1).ok_or("RUN_ID_REQUIRED stage=arguments")?;
    if args[0] == "steer" && !id.starts_with("run-") {
        if args.len() < 3 || !(args.len() - 3).is_multiple_of(2) {
            return Err(
                "SEND_ARGUMENTS stage=arguments: kah steer SESSION TEXT [--receipt ID] [--supersedes RECEIPT]".into(),
            );
        }
        let mut receipt = None;
        let mut supersedes = None;
        for pair in args[3..].chunks_exact(2) {
            match pair[0].as_str() {
                "--receipt" => receipt = Some(pair[1].as_str()),
                "--supersedes" => supersedes = Some(pair[1].as_str()),
                _ => return Err("SEND_ARGUMENTS stage=arguments".into()),
            }
        }
        return if let Some(target) = supersedes {
            crate::send_cli::linked_user(id, &args[2], receipt, target)
        } else {
            crate::send_cli::user(id, &args[2], receipt)
        };
    }
    match args[0].as_str() {
        "steer" if args.len()==3=>{
            let sequence=commands::steer(&base,id,&args[2])?;
            println!("Steering {sequence} queued for {id}.");
        }
        "stop" if args.len()==2=>{commands::stop(&base,id)?;println!("Stop requested for {id}.");}
        "logs" if args.len()==2=>print!("{}",logs(&base,id)?),
        "status" if args.len()==2 || args.len()==3 && args[2]=="--json"=>{
            let record=commands::view(&base,id)?;
            if args.len()==3 {println!("{}",serde_json::to_string_pretty(&record.public_value()?)?);}
            else {print_status(id,&record);}
        }
        _=>return Err("RUN_ARGUMENTS stage=arguments: use kah steer RUN TEXT, kah stop RUN, kah logs RUN, or kah status RUN [--json]".into()),
    }
    Ok(())
}
pub fn print_status(id: &str, record: &Record) {
    println!("{id} — {:?}", record.published_status());
    println!("Task: {}", record.control.task());
    if let Some(native) = &record.native {
        if let Some(name) = &native.name {
            println!("Name: {name}");
        }
        println!(
            "Native {} session: {}; pane: {}",
            native.provider,
            native.session.as_deref().unwrap_or("starting"),
            native.pane.as_deref().unwrap_or("starting")
        );
        if let Some(reason) = &native.waiting
            && record.control.status() == kah_core::unattended::RunStatus::Running
        {
            println!("Needs attention: {reason}");
        }
    }
    println!("Branch: kah/{id}; project: {}", record.project.display());
    println!(
        "Usage: {} tokens; {} ms",
        record.control.tokens_used(),
        record.control.elapsed_ms()
    );
    if let Some(commit) = record.control.last_checkpoint() {
        println!("Checkpoint: {commit}");
    }
    if let Some(question) = record.control.external_question() {
        println!("Needs user action: {question}");
    }
}
fn logs(base: &std::path::Path, id: &str) -> Result<String> {
    let record = commands::view(base, id)?;
    if let Some(native) = record.native {
        let pane = crate::run_pane::Pane {
            id: native.pane.ok_or("RUN_PANE_PENDING stage=run_logs")?,
        };
        let socket = native.herdr_socket.ok_or(
            "RUN_PANE_SERVER_UNKNOWN stage=run_logs: read the saved report or native transcript",
        )?;
        return pane.logs(&native.provider, native.session.as_deref(), Some(&socket));
    }
    Ok(commands::logs(base, id)?)
}
