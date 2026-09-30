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
        if args.len() != 3 && !(args.len() == 5 && args[3] == "--receipt") {
            return Err(
                "SEND_ARGUMENTS stage=arguments: kah steer SESSION TEXT [--receipt ID]".into(),
            );
        }
        return crate::send_cli::user(id, &args[2], args.get(4).map(String::as_str));
    }
    match args[0].as_str() {
        "steer" if args.len()==3=>{
            let sequence=commands::steer(&base,id,&args[2])?;
            println!("Steering {sequence} queued for {id}.");
        }
        "stop" if args.len()==2=>{commands::stop(&base,id)?;println!("Stop requested for {id}.");}
        "logs" if args.len()==2=>print!("{}",commands::logs(&base,id)?),
        "status" if args.len()==2 || args.len()==3 && args[2]=="--json"=>{
            let record=commands::view(&base,id)?;
            if args.len()==3 {println!("{}",serde_json::to_string_pretty(&record)?);}
            else {print_status(id,&record);}
        }
        _=>return Err("RUN_ARGUMENTS stage=arguments: use kah steer RUN TEXT, kah stop RUN, kah logs RUN, or kah status RUN [--json]".into()),
    }
    Ok(())
}
pub fn print_status(id: &str, record: &Record) {
    println!("{id} — {:?}", record.control.status());
    println!("Task: {}", record.control.task());
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
