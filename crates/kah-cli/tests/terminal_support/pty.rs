#[path = "input.rs"]
pub mod input;
use std::{
    fs,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub struct Terminal {
    child: Child,
    input: ChildStdin,
    log: PathBuf,
    cursor_replies: usize,
    tag: String,
}
impl Terminal {
    pub fn start(command: &str, log: &Path) -> Self {
        // A resumed terminal may reuse this fixture log. Clear it before spawn
        // so readiness checks cannot consume the previous process's prompt.
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(log)
            .unwrap();
        fs::set_permissions(log, fs::Permissions::from_mode(0o600)).unwrap();
        let tag = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut child = Command::new("/usr/bin/script")
            .process_group(0)
            .env("KAH_LIVE_FIXTURE", &tag)
            .args([
                "--quiet",
                "--return",
                "--flush",
                "--output-limit",
                "8M",
                "--command",
                &format!("stty cols 120 rows 36; {command}"),
            ])
            .arg(log)
            .env("TERM", "xterm-256color")
            .env_remove("HERDR_ENV")
            .env_remove("CODEX_THREAD_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        Self {
            child,
            input,
            log: log.into(),
            cursor_replies: 0,
            tag,
        }
    }

    pub fn send(&mut self, input: &str) {
        let output = fs::read(&self.log).expect("FIXTURE_TERMINAL_READ stage=input: abort");
        input::send(&output, input, &mut self.input).unwrap();
    }

    pub fn submit(&mut self, text: &str) {
        self.send(text);
        // Native paste-burst detection otherwise treats a same-burst Enter as
        // part of the pasted text rather than dispatching the slash command.
        thread::sleep(Duration::from_millis(500));
        self.send("\r");
    }

    pub fn output(&self) -> String {
        String::from_utf8_lossy(&fs::read(&self.log).unwrap_or_default()).into_owned()
    }

    pub fn wait_for(&mut self, text: &str) {
        self.wait_for_since(text, 0);
    }

    pub fn log_size(&self) -> usize {
        fs::metadata(&self.log)
            .map(|m| m.len() as usize)
            .unwrap_or(0)
    }

    pub fn wait_for_since(&mut self, text: &str, offset: usize) {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            self.respond_to_cursor_queries();
            let bytes = fs::read(&self.log).unwrap_or_default();
            if String::from_utf8_lossy(bytes.get(offset..).unwrap_or_default()).contains(text) {
                return;
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "terminal exited before {text}: {}",
                self.output()
            );
            assert!(
                Instant::now() < deadline,
                "terminal did not show {text}: {}",
                self.output()
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn respond_to_cursor_queries(&mut self) {
        let queries = self.output().matches("\x1b[6n").count();
        while self.cursor_replies < queries {
            self.send("\x1b[1;1R");
            self.cursor_replies += 1;
        }
    }

    pub fn close(&mut self) {
        self.send("\x04");
        self.wait_exit(true);
    }

    pub fn wait_exit(&mut self, require_success: bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(!require_success || status.success(), "{}", self.output());
                return;
            }
            assert!(Instant::now() < deadline, "native TUI did not exit");
            thread::sleep(Duration::from_millis(100));
        }
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let thread = std::thread::current();
            let name = thread
                .name()
                .unwrap_or("unnamed")
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>();
            let diagnostic = format!("/tmp/kah-terminal-failure-{name}-{}.log", self.tag);
            let _ = fs::copy(&self.log, &diagnostic);
            let _ = fs::set_permissions(&diagnostic, fs::Permissions::from_mode(0o600));
            eprintln!("terminal diagnostic: {diagnostic}");
        }
        super::processes::reap(self.child.id(), &self.tag);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[allow(dead_code)] // Shared module; only command-based provider fixtures use this.
pub fn command_line(command: &Command) -> String {
    let mut parts = vec!["env".to_owned()];
    for (key, _) in command.get_envs().filter(|(_, v)| v.is_none()) {
        parts.extend(["-u".into(), quote(key.to_str().unwrap())]);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            parts.push(quote(&format!(
                "{}={}",
                key.to_str().unwrap(),
                value.to_str().unwrap()
            )));
        }
    }
    parts.push(quote(command.get_program().to_str().unwrap()));
    parts.extend(command.get_args().map(|a| quote(a.to_str().unwrap())));
    format!(
        "cd {} && {}",
        quote(command.get_current_dir().unwrap().to_str().unwrap()),
        parts.join(" ")
    )
}
