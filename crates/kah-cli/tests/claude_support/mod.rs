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
#[allow(clippy::duplicate_mod)]
#[path = "../terminal_support/claude_observer.rs"]
pub mod observer;
pub mod settings;
pub mod subagent_completion;
mod transcript;

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
        Self::new_domain("mine")
    }
    pub fn new_domain(domain: &str) -> Self {
        assert!(["mine", "research", "untrusted"].contains(&domain));
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let id = format!("00000000-0000-4000-8000-{:012x}", nonce & 0xffffffffffff);
        let root = home.join(format!("repos/{domain}/kah-claude-live-{nonce}"));
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
        let transcript_directory = kah_claude::paths::transcript(&home, &root, &id)
            .parent()
            .unwrap()
            .to_owned();
        Self {
            created_root: root.clone(),
            root,
            id,
            base: RunStore::default_state_root().unwrap(),
            evidence: PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()),
            transcript_directory,
        }
    }
    pub fn start(&self, resume: bool) -> Terminal {
        self.start_with(resume, "claude-sonnet-5-5", "low", &[])
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
            format!(
                "--model {} --effort {} {}",
                quote(model),
                quote(effort),
                observer::shell()
            )
        };
        let extra = extra.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ");
        let command = format!(
            "cd {} && {}{} --agent claude {} -- {} {}",
            quote(self.root.to_str().unwrap()),
            inherited_claude_unset(),
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
    /// Move a native picker's `❯` cursor up to the row whose label starts with
    /// `label`; rows are found by text, never by number.
    pub fn move_cursor_to(&self, terminal: &mut Terminal, label: &str, offset: usize) {
        for _ in 0..12 {
            terminal.respond_to_cursor_queries();
            let output = terminal.output();
            if output.len() > offset && cursor_on(&screen(&output), label) {
                return;
            }
            terminal.send("\x1b[A");
            std::thread::sleep(Duration::from_millis(250));
        }
        panic!("native picker cursor never reached {label}");
    }
    pub fn rows(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(transcript::path(self))
            .unwrap_or_default()
            .lines()
            .filter_map(|s| serde_json::from_str(s).ok())
            .collect()
    }
    pub fn child_transcripts(&self) -> usize {
        fs::read_dir(
            transcript::path(self)
                .parent()
                .unwrap()
                .join(&self.id)
                .join("subagents"),
        )
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
        self.wait(terminal, || self.is_settled(text));
    }
    pub fn is_settled(&self, text: &str) -> bool {
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
                    .is_some_and(|i| completion::completed_after(&self.rows(), &i.source.record))
            })
    }
}

/// A shell started inside Claude Code exports its own session identity, e.g.
/// CLAUDE_CODE_CHILD_SESSION, which stops the fixture writing a transcript.
/// Drop every inherited CLAUDE* name except the user's config root.
pub(crate) fn inherited_claude_names() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("CLAUDE") && name != "CLAUDE_CONFIG_DIR")
        .collect()
}
/// Shell prefix form of [`inherited_claude_names`] for command strings.
pub(crate) fn inherited_claude_unset() -> String {
    let names = inherited_claude_names()
        .iter()
        .map(|name| format!("-u {name} "))
        .collect::<String>();
    if names.is_empty() {
        String::new()
    } else {
        format!("env {names}")
    }
}

/// The flattened screen shows the cursor row as `❯ 2. Opus 5.5 ...`.
fn cursor_on(flat: &str, label: &str) -> bool {
    flat.split("❯ ").skip(1).any(|rest| {
        rest.trim_start_matches(|c: char| c.is_ascii_digit())
            .strip_prefix(". ")
            .is_some_and(|row| row.starts_with(label))
    })
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
        {
            if let Ok(project) = kah_sandbox::Project::select(
                &PathBuf::from(std::env::var_os("HOME").unwrap()).join("repos"),
                &self.root,
                None,
            ) && let Ok(runtime) = kah_cli::claude_setup::runtime_directory(&project)
            {
                let _ = fs::remove_dir_all(runtime);
            }
        }
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
