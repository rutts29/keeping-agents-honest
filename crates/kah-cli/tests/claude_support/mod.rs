use crate::pty::{Terminal, quote};
use kah_core::run_store::RunStore;
use kah_observer::session::Store;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub struct Fixture {
    pub root: PathBuf,
    pub id: String,
    pub base: PathBuf,
    pub evidence: PathBuf,
    transcript_directory: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let id = format!("00000000-0000-4000-8000-{:012x}", nonce & 0xffffffffffff);
        let root = home.join(format!("repos/mine/kah-claude-live-{nonce}"));
        fs::create_dir(&root).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(&root)
                .status()
                .unwrap()
                .success()
        );
        let encoded = root.to_string_lossy().replace('/', "-");
        Self {
            root,
            id,
            base: RunStore::default_state_root().unwrap(),
            evidence: PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()),
            transcript_directory: home.join(".claude/projects").join(encoded),
        }
    }
    pub fn start(&self, resume: bool) -> Terminal {
        let native = if resume {
            format!("--resume {}", self.id)
        } else {
            format!("--session-id {} --permission-mode acceptEdits", self.id)
        };
        let command = format!(
            "cd {} && {} --agent claude --model haiku --effort low -- {}",
            quote(self.root.to_str().unwrap()),
            quote(env!("CARGO_BIN_EXE_kah")),
            native
        );
        Terminal::start(&command, &self.root.join("terminal.log"))
    }
    pub fn ready(&self, terminal: &mut Terminal) {
        let mut trusted = false;
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            terminal.respond_to_cursor_queries();
            let text = screen(&terminal.output());
            if !trusted && text.contains("Yes, I trust this folder") {
                // Installed 2.1.285 remounts the selector after its 150 ms
                // confirmation guard; wait for that guard before selecting.
                std::thread::sleep(Duration::from_millis(350));
                terminal.submit("\x1b[B");
                trusted = true;
            }
            if terminal.output().contains("\x1b[?1049h")
                && Store::read(&self.base, &self.id).ok().flatten().is_some()
            {
                return;
            }
            assert!(Instant::now() < deadline, "Claude startup timed out");
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    pub fn wait(&self, terminal: &mut Terminal, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(120);
        while !condition() {
            terminal.respond_to_cursor_queries();
            assert!(
                Instant::now() < deadline,
                "Claude fixture timed out; inspect its private terminal log"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn marker(&self, text: &str) -> bool {
        fs::read_to_string(self.root.join("marker.txt")).is_ok_and(|s| s.trim() == text)
    }
    pub fn prompt(&self, terminal: &mut Terminal, text: &str) {
        // Claude's own PTY reply path uses bracketed paste, then Enter.
        terminal.submit(&format!("\x1b[200~{text}\x1b[201~"));
    }
    pub fn settled(&self, terminal: &mut Terminal, text: &str) {
        self.wait(terminal, || {
            let Ok(Some(state)) = Store::read(&self.base, &self.id) else {
                return false;
            };
            if state.inflight.is_some() || state.gap.is_some() {
                return false;
            }
            Store::open(&self.base, &self.id, Duration::ZERO)
                .and_then(|s| s.sources())
                .is_ok_and(|sources| {
                    sources
                        .iter()
                        .any(|i| i.text.contains(text) && i.decision.is_some())
                })
        });
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn screen(raw: &str) -> String {
    let mut text = String::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            text.push(' ');
        } else {
            text.push(ch);
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(Some(state)) = Store::read(&self.base, &self.id)
            && let Some(run) = state.run_id
            && let Ok(path) = RunStore::path_for(&self.base, &self.root, run)
        {
            let _ = fs::remove_dir_all(path);
        }
        let _ = fs::remove_dir_all(Store::directory(&self.base, &self.id));
        let cleanup = Command::new("claude")
            .args(["project", "purge", "--yes"])
            .arg(&self.root)
            .output();
        if !cleanup.is_ok_and(|o| o.status.success()) {
            eprintln!("Claude fixture project-state purge failed");
        }
        let _ = fs::remove_dir_all(&self.transcript_directory);
        let _ = fs::remove_dir_all(&self.root);
    }
}
