use std::{
    fs,
    io::Write,
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
}
impl Terminal {
    pub fn start(command: &str, log: &Path) -> Self {
        // A resumed terminal may reuse this fixture log. Clear it before spawn
        // so readiness checks cannot consume the previous process's prompt.
        fs::File::create(log).unwrap();
        let mut child = Command::new("/usr/bin/script")
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
        }
    }

    pub fn send(&mut self, input: &str) {
        self.input.write_all(input.as_bytes()).unwrap();
        self.input.flush().unwrap();
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
            let output = self.output();
            let queries = output.matches("\x1b[6n").count();
            while self.cursor_replies < queries {
                self.send("\x1b[1;1R");
                self.cursor_replies += 1;
            }
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
            let diagnostic = format!("/tmp/kah-terminal-failure-{}.log", std::process::id());
            let _ = fs::copy(&self.log, &diagnostic);
            eprintln!("terminal diagnostic: {diagnostic}");
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
