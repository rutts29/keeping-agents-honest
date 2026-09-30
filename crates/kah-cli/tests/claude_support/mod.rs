use crate::pty::{Terminal, quote};
use kah_core::run_store::RunStore;
use kah_observer::session::Store;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
mod cleanup;
mod completion;

pub struct Fixture {
    root: PathBuf,
    created_root: PathBuf,
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
        let root = root.canonicalize().unwrap();
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
            created_root: root.clone(),
            root,
            id,
            base: RunStore::default_state_root().unwrap(),
            evidence: PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()),
            transcript_directory: home.join(".claude/projects").join(encoded),
        }
    }
    pub fn start(&self, resume: bool) -> Terminal {
        self.start_with(resume, "haiku", "low", &[])
    }
    pub fn start_with(&self, resume: bool, model: &str, effort: &str, extra: &[&str]) -> Terminal {
        let native = if resume {
            format!("--resume {} --permission-mode acceptEdits", self.id)
        } else {
            format!(
                "--session-id {} --permission-mode acceptEdits --allowedTools Bash",
                self.id
            )
        };
        let choices = if resume {
            String::new()
        } else {
            format!("--model {} --effort {}", quote(model), quote(effort))
        };
        let extra = extra.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ");
        let command = format!(
            "cd {} && {} --agent claude {} -- {} {}",
            quote(self.root.to_str().unwrap()),
            quote(env!("CARGO_BIN_EXE_kah")),
            choices,
            native,
            extra
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
            if text.contains("Claude Code v")
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
    pub fn wait_text(&self, terminal: &mut Terminal, text: &str, offset: usize) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            terminal.respond_to_cursor_queries();
            let output = terminal.output();
            if output.len() > offset && screen(&output).contains(text) {
                return;
            }
            assert!(Instant::now() < deadline, "native UI did not show {text}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn rows(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.transcript_directory.join(format!("{}.jsonl", self.id)))
            .unwrap_or_default()
            .lines()
            .filter_map(|s| serde_json::from_str(s).ok())
            .collect()
    }
    pub fn child_transcripts(&self) -> usize {
        fs::read_dir(self.transcript_directory.join(&self.id).join("subagents"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|s| s == "jsonl"))
                    .count()
            })
            .unwrap_or(0)
    }
    pub fn has_file(&self, file: &str, text: &str) -> bool {
        fs::read_to_string(self.root.join(file)).is_ok_and(|s| s.trim() == text)
    }
    pub fn project(&self) -> &std::path::Path {
        &self.root
    }
    pub fn reserve_for_creation(&self) {
        assert!(cleanup::command(&self.root, &self.created_root).is_ok());
        fs::remove_dir_all(self.root.join(".git")).unwrap();
        fs::remove_dir(&self.root).unwrap();
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
                        .rev()
                        .find(|i| i.text.contains(text) && i.decision.is_some())
                        .is_some_and(|i| {
                            completion::completed_after(&self.rows(), &i.source.record)
                        })
                })
        });
    }
}

fn screen(raw: &str) -> String {
    let mut parser = vt100::Parser::new(36, 120, 0);
    parser.process(raw.as_bytes());
    parser
        .screen()
        .contents()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let Ok(mut purge) = cleanup::command(&self.root, &self.created_root) else {
            eprintln!("Claude fixture cleanup refused: created project identity changed");
            return;
        };
        if let Ok(Some(state)) = Store::read(&self.base, &self.id)
            && let Some(run) = state.run_id
            && let Ok(path) = RunStore::path_for(&self.base, &self.root, run)
        {
            let _ = fs::remove_dir_all(path);
        }
        let _ = fs::remove_dir_all(Store::directory(&self.base, &self.id));
        let cleanup = purge.output();
        if !cleanup.is_ok_and(|o| o.status.success()) {
            eprintln!("Claude fixture project-state purge failed");
        }
        let _ = fs::remove_dir_all(&self.transcript_directory);
        let _ = fs::remove_dir_all(&self.root);
    }
}
