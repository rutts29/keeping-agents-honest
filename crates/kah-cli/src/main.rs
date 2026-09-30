use kah_cli::{Arguments, Launch, Result};
use std::{env, os::unix::process::CommandExt, path::PathBuf};

fn run() -> Result<()> {
    let arguments = kah_cli::shell_routing::route(env::args().skip(1).collect())?;
    if arguments.first().map(String::as_str) == Some("__claude-hook") {
        kah_cli::claude_hook::run();
        return Ok(());
    }
    if arguments == ["smoke"] {
        let home = PathBuf::from(env::var_os("HOME").ok_or("HOME_MISSING stage=smoke")?);
        return kah_cli::smoke::run(&home);
    }
    if arguments.first().is_some_and(|s| s == "send") {
        return kah_cli::send_cli::run(&arguments);
    }
    if arguments.first().is_some_and(|s| s == "eval") {
        return kah_cli::evaluation_cli::run(&arguments);
    }
    if arguments.first().is_some_and(|s| s == "__fetch") {
        return kah_cli::fetch_cli::run(&arguments);
    }
    if kah_cli::unattended_run::handles(&arguments) {
        return kah_cli::unattended_run::run_cli(arguments);
    }
    if arguments.first().is_some_and(|s| s == "__attach-observer") {
        return attach(&arguments);
    }
    if kah_cli::run_commands::handles(&arguments) {
        return kah_cli::run_commands::run(arguments);
    }
    if matches!(
        arguments.first().map(String::as_str),
        Some("status" | "ledger")
    ) {
        return kah_cli::inspection_cli::run(arguments);
    }
    if arguments.first().map(String::as_str) == Some("__hook") {
        kah_cli::observer_hook::run();
        return Ok(());
    }
    if arguments.first().map(String::as_str) == Some("__herdr-session") {
        kah_cli::herdr::report();
        return Ok(());
    }
    let args = Arguments::parse(arguments)?;
    if args.help {
        println!(
            "kah [--agent codex|claude] [mine|research|untrusted PROJECT] [--online] [--yolo] [--model MODEL] [--effort EFFORT] [-- NATIVE_ARGS]\n\nStart native Codex, or Claude Code in Mine, in the current project.\nClaude resume: kah --agent claude -- --resume SESSION (from its Mine project).\nRestricted/unattended Claude is unavailable pending a proven sandbox boundary.\nResearch and Untrusted commands use gVisor; Research is offline unless --online.\nMine uses native workspace permissions; --yolo is Mine-only.\nNative resume: kah -- resume [SESSION] --all\nObserver: automatic, Jev in every domain; --no-observer opts out.\nUnattended: kah run --test COMMAND [--model MODEL --effort EFFORT] [--budget-tokens N --time-limit SECONDS] TASK\nControls: kah steer RUN TEXT; kah status RUN; kah logs RUN; kah stop RUN; kah resume RUN."
        );
        return Ok(());
    }
    launch(args)
}

fn launch(args: Arguments) -> Result<()> {
    let home =
        PathBuf::from(env::var_os("HOME").ok_or("HOME_MISSING stage=launch: HOME is unset")?);
    let cwd = env::current_dir().map_err(|e| format!("CWD_READ stage=launch: {e}"))?;
    if args.agent.as_deref() == Some("claude") {
        eprintln!("kah: Claude Code with project policy");
        return Err(format!(
            "CLAUDE_EXEC stage=launch: {}",
            kah_cli::claude_launch::prepare(&home, &cwd, args)?.exec()
        )
        .into());
    }
    if args.agent.as_deref().is_some_and(|s| s != "codex") {
        return Err("AGENT_UNSUPPORTED stage=arguments: choose codex or claude".into());
    }
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

fn attach(arguments: &[String]) -> Result<()> {
    let session = arguments
        .get(1)
        .filter(|_| matches!(arguments.len(), 2 | 3))
        .ok_or("ATTACH_SESSION_REQUIRED stage=arguments")?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME_MISSING stage=attach")?);
    let timeout = arguments
        .get(2)
        .map(|s| s.parse::<u64>())
        .transpose()
        .map_err(|_| "ATTACH_TIMEOUT stage=arguments")?
        .unwrap_or(120);
    kah_cli::attached_observer::watch(&home, &env::current_dir()?, session, timeout)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kah: {error}");
        std::process::exit(1);
    }
}
