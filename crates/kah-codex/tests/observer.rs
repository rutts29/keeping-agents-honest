use kah_codex::observer::{Environment, Request};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[test]
fn observer_command_uses_explicit_choices_ephemeral_history_and_a_process_deadline() {
    let environment = Environment {
        home: PathBuf::from("/private/observer"),
        workspace: PathBuf::from("/public/empty"),
        native: PathBuf::from("/native/codex"),
    };
    let request = Request {
        model: "chosen-model",
        effort: "chosen-effort",
        schema: Path::new("/private/schema"),
        output: Path::new("/private/output"),
        timeout: Duration::from_secs(20),
    };
    let command = environment.command(&request).unwrap();
    assert_eq!(command.get_program(), "/usr/bin/timeout");
    let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
    assert!(args.contains(&"--ephemeral"));
    assert!(args.contains(&"20s"));
    assert!(args.windows(2).any(|p| p == ["--model", "chosen-model"]));
    assert!(
        args.windows(2)
            .any(|p| p == ["--sandbox", "danger-full-access"])
    );
    assert!(args.windows(2).any(|p| p == ["-C", "/public/empty"]));
    assert!(!args.contains(&"--foreground"));
    for name in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "HERDR_ENV",
        "CODEX_THREAD_ID",
    ] {
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == name && value.is_none())
        );
    }
    let unlimited = Request {
        timeout: Duration::ZERO,
        ..request
    };
    assert_eq!(
        environment.command(&unlimited).unwrap_err().code,
        "OBSERVER_TIMEOUT_CONFIG"
    );
}
