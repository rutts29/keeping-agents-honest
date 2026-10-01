use crate::Result;
use std::path::{Path, PathBuf};
pub fn run(args: &[String]) -> Result<()> {
    if args.len() == 3 && args[1] == "completion" {
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("EVAL_HOME stage=arguments")?);
        let path = crate::completion_replay::run(&home, Path::new(&args[2]))?;
        println!(
            "Completion replay: {}; policy and active runs unchanged.",
            path.display()
        );
        return Ok(());
    }
    if !matches!(args.len(), 3 | 4)
        || args[1] != "replay"
        || args.get(3).is_some_and(|a| a != "--validation")
    {
        return Err(
            "EVAL_ARGUMENTS stage=arguments: kah eval replay DATASET.jsonl [--validation]".into(),
        );
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("EVAL_HOME stage=arguments")?);
    let report = kah_observer::evaluation::run(&home, Path::new(&args[2]), args.len() == 4)?;
    println!(
        "Replay report: {}\nPolicy and thresholds unchanged.",
        report.display()
    );
    Ok(())
}
