//! Full native unattended session entrypoints.
use crate::Result;
use std::path::PathBuf;

pub fn handles(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "run")
        || args.first().is_some_and(|s| s == "resume")
            && args.get(1).is_some_and(|s| s.starts_with("run-"))
}
pub fn run_cli(args: Vec<String>) -> Result<()> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME_MISSING stage=run_arguments")?);
    if args[0] == "resume" {
        if args.len() != 2 {
            return Err("RUN_RESUME_ARGUMENTS stage=arguments: kah resume RUN".into());
        }
        return crate::native_controller::launch(&home, &args[1], true);
    }
    let options = crate::unattended_args::RunArguments::parse(args.into_iter().skip(1).collect())?;
    let cwd = std::env::current_dir().map_err(|e| format!("RUN_CWD stage=run_arguments: {e}"))?;
    let id = crate::unattended_setup::create(&home, &cwd, options)?;
    println!("Created {id}");
    crate::native_controller::launch(&home, &id, false)
}
