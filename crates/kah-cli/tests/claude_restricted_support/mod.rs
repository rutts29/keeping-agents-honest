use crate::{
    claude_support::Fixture,
    pty::{Terminal, quote},
};
use kah_observer::{judge::routing::Backend, session::Binding};
use kah_sandbox::{Domain, Executor, LaunchOptions, Project, current_uid, publish_binary};
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

pub struct Restricted {
    pub fixture: Fixture,
    pub home: PathBuf,
    pub canary: PathBuf,
    private: PathBuf,
    prefix: PathBuf,
    temporary: PathBuf,
}
impl Restricted {
    pub fn new() -> Self {
        let fixture = Fixture::new_domain("research");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let private = PathBuf::from(format!("/var/tmp/kah-claude-prefix-{}", fixture.id));
        fs::create_dir(&private).unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
        let temporary = private.join("tmp");
        fs::create_dir(&temporary).unwrap();
        let canary = private.join("sensitive.txt");
        fs::write(&canary, "PRIVATE_CANARY_CONTENT").unwrap();
        fs::write(fixture.project().join("input.txt"), "ALPHA\n").unwrap();
        std::os::unix::fs::symlink(&canary, fixture.project().join("denied-link.txt")).unwrap();
        fs::write(
            fixture.project().join("fixture-server.js"),
            include_str!("../native_extensions_support/server.js"),
        )
        .unwrap();
        let prefix = private.join("prefix");
        let result = Self {
            fixture,
            home,
            canary,
            private,
            prefix,
            temporary,
        };
        result.prepare_prefix();
        result
    }
    fn prepare_prefix(&self) {
        let uid = kah_run::access::service_uid().unwrap();
        kah_run::access::traverse(&self.private, uid).unwrap();
        kah_run::access::share_tree(&self.temporary, uid, true).unwrap();
        kah_run::access::share_tree(self.fixture.project(), uid, true).unwrap();
        let binary = publish_binary(
            &self.home.join(".local/bin/codex"),
            &PathBuf::from(format!("/var/tmp/kah-runtime-{}", current_uid().unwrap())),
        )
        .unwrap();
        let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
        let project = Project {
            domain: Domain::Research,
            root: self.fixture.project().into(),
            relative_cwd: Default::default(),
            needs_creation: false,
        };
        let mut executor = Executor::new(
            &project,
            LaunchOptions::default(),
            &binary,
            image.trim(),
            uid,
        )
        .unwrap();
        for arg in &mut executor.args {
            if arg == "0:0" {
                *arg = "65534:65534".into();
            } else if arg == "/home/sandbox:rw,nosuid,nodev,size=512m,mode=700" {
                arg.push_str(",uid=65534,gid=65534");
            }
        }
        executor
            .add_writable_mount(&self.temporary, &self.temporary)
            .unwrap();
        let spec = self.private.join("control/executor.json");
        kah_cli::sandbox_command::Spec {
            executor,
            temporary: Some(self.temporary.clone()),
            service_uid: uid,
            environment: vec![],
        }
        .save(&spec)
        .unwrap();
        let command = format!(
            "{} --sandbox-shell {}",
            quote(env!("CARGO_BIN_EXE_kah")),
            quote(spec.to_str().unwrap())
        );
        fs::write(&self.prefix, format!("#!/bin/sh\nexec {command} \"$1\"\n")).unwrap();
        fs::set_permissions(&self.prefix, fs::Permissions::from_mode(0o700)).unwrap();
    }
    pub fn verify_cwd_file(&self) {
        let path = self.temporary.join("cwd-probe");
        fs::write(&path, "pending").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let output = Command::new(&self.prefix)
            .arg(format!(
                "printf PREFIX_OK > {}",
                quote(path.to_str().unwrap())
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "prefix temp-file probe: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "PREFIX_OK");
    }
    pub fn start(&self) -> Terminal {
        let binding = Binding {
            session: String::new(),
            project: self.fixture.project().into(),
            codex_home: self.home.join(".codex"),
            native: self.home.join(".local/bin/codex"),
            model: "gpt-6.1-sol".into(),
            effort: "low".into(),
            timeout_secs: 45,
            judge: Backend::Jev,
            jev_model: "jev-latest".into(),
        };
        let mut settings = json!({"permissions":{"defaultMode":"acceptEdits","allow":["Bash","mcp__fixture__probe"],"deny":self.denies()},"hooks":{}});
        for event in ["SessionStart", "UserPromptSubmit", "PreToolUse", "Stop"] {
            settings["hooks"][event] = json!([{"hooks":[{"type":"command","command":self.home.join(".local/bin/kah"),"args":["__claude-hook"],"timeout":120}]}]);
        }
        let hook = format!(
            "test ! -e {} && test ! -e /home/example/.claude/.credentials.json && printf HOOK_GVISOR > hook-proof.txt",
            quote(self.canary.to_str().unwrap())
        );
        settings["hooks"]["SessionStart"]
            .as_array_mut()
            .unwrap()
            .push(json!({"hooks":[{"type":"command","command":hook}]}));
        let mcp = json!({"mcpServers":{"fixture":{"command":"/usr/bin/node","args":[self.fixture.project().join("fixture-server.js"),self.canary]}}});
        let command = format!(
            "cd {} && env -u CODEX_THREAD_ID KAH_CLAUDE_BINDING={} CLAUDE_CODE_SHELL_PREFIX={} CLAUDE_CODE_TMPDIR={} {} --model claude-sonnet-5-5 --effort low --session-id {} --setting-sources '' --settings {} --strict-mcp-config --mcp-config {}",
            quote(self.fixture.project().to_str().unwrap()),
            quote(&serde_json::to_string(&binding).unwrap()),
            quote(self.prefix.to_str().unwrap()),
            quote(self.temporary.to_str().unwrap()),
            quote(self.home.join(".local/bin/claude").to_str().unwrap()),
            quote(&self.fixture.id),
            quote(&settings.to_string()),
            quote(&mcp.to_string())
        );
        Terminal::start(&command, &self.fixture.project().join("terminal.log"))
    }
    fn denies(&self) -> Vec<String> {
        let mut paths = vec![self.private.clone()];
        paths.extend([".ssh", ".codex", ".claude", ".config"].map(|p| self.home.join(p)));
        let mut denies = vec!["WebFetch".into()];
        for path in paths {
            for tool in ["Read", "Edit"] {
                denies.push(format!("{tool}(/{}/**)", path.display()));
            }
        }
        denies
    }
}
impl Drop for Restricted {
    fn drop(&mut self) {
        let uid = kah_run::access::service_uid().unwrap();
        let socket = format!("unix:///run/user/{uid}/docker.sock");
        let label = format!("label=dev.kah.project={}", self.fixture.project().display());
        if let Ok(output) = Command::new("docker")
            .args(["--host", &socket, "ps", "-aq", "--filter", &label])
            .output()
        {
            for id in String::from_utf8_lossy(&output.stdout).split_whitespace() {
                let _ = Command::new("docker")
                    .args(["--host", &socket, "rm", "-f", id])
                    .output();
            }
        }
        let _ = fs::remove_dir_all(&self.private);
    }
}
