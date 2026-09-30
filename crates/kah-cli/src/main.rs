use kah_cli::{Arguments, Launch, Result};
use std::{env, os::unix::process::CommandExt, path::PathBuf};

fn run() -> Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if kah_cli::run_commands::handles(&arguments) {
        return kah_cli::run_commands::run(arguments);
    }
    if matches!(env::args().nth(1).as_deref(), Some("status" | "ledger")) {
        return kah_cli::inspection_cli::run(env::args().skip(1).collect());
    }
    if env::args().nth(1).as_deref() == Some("__hook") {
        kah_cli::observer_hook::run();
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("__herdr-session") {
        kah_cli::herdr::report();
        return Ok(());
    }
    let args = Arguments::parse(env::args().skip(1).collect())?;
    if args.help {
        println!(
            "kah [mine|research|untrusted PROJECT] [--online] [--yolo] [--model MODEL] [--effort EFFORT] [-- NATIVE_ARGS]\n\nStart native Codex in the current project or create a named project.\nResearch and Untrusted commands use gVisor; Research is offline unless --online.\nMine uses native workspace permissions; --yolo is Mine-only.\nNative resume: kah -- resume [SESSION] --all\nObserver integration is still under development (Phase 3)."
        );
        return Ok(());
    }
    let home =
        PathBuf::from(env::var_os("HOME").ok_or("HOME_MISSING stage=launch: HOME is unset")?);
    let cwd = env::current_dir().map_err(|e| format!("CWD_READ stage=launch: {e}"))?;
    let launch = Launch::prepare(&home, &cwd, args)?;
    eprintln!(
        "kah: {} project {}{}",
        launch.project.domain.as_str(),
        launch.project.root.display(),
        if launch.environment_id.is_some() {
            " (gVisor executor)"
        } else {
            ""
        }
    );
    Err(format!("CODEX_EXEC stage=launch: {}", launch.command(false).exec()).into())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kah: {error}");
        std::process::exit(1);
    }
}
