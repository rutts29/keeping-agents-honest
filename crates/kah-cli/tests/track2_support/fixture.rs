#[path = "helper_binary.rs"]
mod helper_binary;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};
pub struct Fixture {
    pub root: PathBuf,
    pub home: PathBuf,
    pub name: String,
    pub dry: bool,
    protected: Vec<(PathBuf, String)>,
    logins: Vec<(PathBuf, PathBuf)>,
    config_before: Value,
}
impl Fixture {
    pub fn new(mode: &str) -> Self {
        let actual = PathBuf::from(std::env::var_os("HOME").unwrap());
        let name = format!("kah-gate-track2-{}", kah_observer::session::now_ms());
        let root = PathBuf::from("/mnt/kah-fixtures/cargo/test-fixtures").join(&name);
        kah_core::run_store::ensure_private_directory(&root).unwrap();
        let uid = kah_run::access::service_uid().unwrap();
        for parent in [&root, root.parent().unwrap()] {
            kah_run::access::traverse(parent, uid).unwrap();
        }
        let home = root.join("home");
        for dir in [
            ".local/bin",
            ".codex/sessions",
            ".codex/archived_sessions",
            ".claude/projects",
            ".cache",
            "repos/mine",
            "repos/research",
        ] {
            fs::create_dir_all(home.join(dir)).unwrap();
        }
        let mut result = Self {
            root,
            home,
            name,
            dry: std::env::var_os("KAH_TRACK2_DRY_RUN").is_some(),
            protected: vec![],
            logins: vec![],
            config_before: config_snapshot(&actual.join(".config/kah")),
        };
        save(
            &result.root.join("config-before.json"),
            &result.config_before,
        );
        result.native_setup(&actual);
        result.sources(&actual, mode);
        result.seed();
        if mode == "proofs" {
            super::proof_inputs::retain_source(&result.root, &actual);
        }
        save(
            &result.root.join("ownership.json"),
            &json!({"name":result.name,"home":result.home,
            "protected":result.protected,"login_links":result.logins,"config_before":result.config_before,"mode":mode,"dry_run":result.dry}),
        );
        result
    }
    fn native_setup(&mut self, actual: &Path) {
        for provider in ["codex", "claude"] {
            symlink(
                actual.join(".local/bin").join(provider),
                self.home.join(".local/bin").join(provider),
            )
            .unwrap();
        }
        let binary = std::env::var_os("KAH_TRACK2_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_kah")));
        helper_binary::install(&binary, &self.home.join(".local/bin/kah"));
        let target = actual.join(".codex/auth.json");
        let link = self.home.join(".codex/auth.json");
        symlink(&target, &link).unwrap();
        self.logins.push((link, target));
        self.logins.push(super::claude_login::link(&self.home));
        // Existing Jev loader rejects a symlink final component. Link its config
        // directory, so native kah reads the original regular key; never copy it.
        fs::create_dir_all(self.home.join(".config")).unwrap();
        let target = actual.join(".config/kah");
        let link = self.home.join(".config/kah");
        symlink(&target, &link).unwrap();
        self.logins.push((link, target));
        for relative in [".codex/config.toml", ".local/bin/kah"] {
            let path = actual.join(relative);
            if path.is_file() {
                self.protected.push((path.clone(), digest(&path)));
            }
        }
        fs::write(
            self.home.join(".codex/config.toml"),
            "model=\"gpt-6.1-sol\"\nmodel_reasoning_effort=\"low\"\nservice_tier=\"default\"\n",
        )
        .unwrap();
        save(
            &self.home.join(".claude/settings.json"),
            &json!({"model":"claude-sonnet-5-5","effortLevel":"low","fastMode":false}),
        );
        save(
            &self.home.join(".claude.json"),
            &json!({"hasCompletedOnboarding":true,"theme":"dark","mcpServers":{}}),
        );
        save(
            &self.root.join("binary.json"),
            &json!({"source":binary,"sha256":digest(&binary)}),
        );
    }
    fn sources(&mut self, actual: &Path, mode: &str) {
        if mode != "long" {
            return;
        }
        let inventory: Value = serde_json::from_slice(
            &fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../docs/gates/evidence/phase-5o-long-session/source-inventory-before.json",
            ))
            .unwrap(),
        )
        .unwrap();
        let mut rows = vec![];
        for row in inventory["inventory"].as_array().unwrap() {
            if row["provider"] == "claude"
                && std::env::var("KAH_TRACK2_DRY_PROVIDER").as_deref() == Ok("codex")
            {
                continue;
            }
            let source = PathBuf::from(row["path"].as_str().unwrap());
            assert!(
                source.starts_with(actual),
                "TRACK2_SOURCE_SCOPE stage=prepare"
            );
            assert!(
                !source.starts_with(actual.join(".claude")),
                "TRACK2_OPERATOR_CLAUDE_PATH_DISABLED stage=sources: a retained test-template session is required"
            );
            let before = digest(&source);
            assert_eq!(
                format!("sha256:{before}"),
                row["digest"],
                "TRACK2_SOURCE_CHANGED stage=prepare"
            );
            let target = if row["provider"] == "codex" {
                self.home.join(source.strip_prefix(actual).unwrap())
            } else {
                self.home
                    .join(".claude/projects")
                    .join(kah_claude::paths::project_key(
                        &self.home.join("repos/mine/fixture-claude"),
                    ))
                    .join(source.file_name().unwrap())
            };
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(&source, &target).unwrap();
            assert_eq!(digest(&target), before);
            self.protected.push((source.clone(), before.clone()));
            rows.push(json!({"provider":row["provider"],"id":row["id"],"source":source,"copy":target,
                "sha256":before,"bytes":fs::metadata(&source).unwrap().len(),"source_mtime_ms":modified_ms(&source),
                "inherited_age_ms":kah_observer::session::now_ms().saturating_sub(modified_ms(&source))}));
        }
        save(&self.root.join("sources.json"), &json!(rows));
    }
    fn seed(&self) {
        let repo = self.home.join("repos/mine/source");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--quiet"]);
        fs::write(repo.join("PLAN.md"),"# Fixture\nPublic marker must remain BLUE. Phase 1 exposes marker and records absent serializer as a genuine Phase 2 dependency. Phase 2 creates serializer, closes dependency and proves integration. Phase 3 documents the earlier marker finding and its repair. No pushes, dependencies, subagents or changes outside this fixture.\n").unwrap();
        fs::write(repo.join("marker.txt"), "BLUE\n").unwrap();
        fs::write(repo.join("check.sh"),"#!/bin/sh\nset -eu\ntest \"$(cat marker.txt)\" = BLUE\nif [ -e serializer.txt ]; then test \"$(cat serializer.txt)\" = SERIALIZER; fi\necho FIXTURE_INTEGRATION_PASS\n").unwrap();
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "fixture baseline",
            ],
        );
        for provider in ["codex", "claude"] {
            let worktree = self.home.join(format!("repos/mine/fixture-{provider}"));
            git(
                &repo,
                &[
                    "clone",
                    "--quiet",
                    "--local",
                    repo.to_str().unwrap(),
                    worktree.to_str().unwrap(),
                ],
            );
            git(&worktree, &["checkout", "--quiet", "-b", "fixture"]);
            git(&worktree, &["config", "user.name", "Fixture"]);
            git(
                &worktree,
                &["config", "user.email", "fixture@example.invalid"],
            );
            git(&worktree, &["config", "commit.gpgsign", "false"]);
        }
        super::trust::preseed_claude(&self.home, &self.home.join("repos/mine/fixture-claude"));
    }
    pub fn audit(&self) -> Value {
        let protected = self
            .protected
            .iter()
            .map(|(path, before)| {
                let after = digest(path);
                json!({"path":path,"before":before,
            "after":after,"unchanged":after==*before})
            })
            .collect::<Vec<_>>();
        let logins = self
            .logins
            .iter()
            .map(|(link, target)| super::claude_login::audit_link(link, target))
            .collect::<Vec<_>>();
        let processes = crate::processes::list()
            .into_iter()
            .filter(|p| p.cwd.starts_with(self.root.to_str().unwrap()))
            .map(|p| json!({"pid":p.pid,"cwd":p.cwd}))
            .collect::<Vec<_>>();
        let passed = processes.is_empty()
            && super::proof_inputs::audit(&self.root)["passed"] == true
            && protected.iter().all(|r| r["unchanged"] == true)
            && logins.iter().all(|r| r["symlink_unchanged"] == true)
            && config_snapshot(&self.home.join(".config/kah")) == self.config_before;
        json!({"passed":passed,"processes":processes,"protected":protected,"login_links":logins,
            "historical_replay_source":super::proof_inputs::audit(&self.root),
            "recorder":{"path":self.root.join("terminal.log"),"bytes":fs::metadata(self.root.join("terminal.log")).map(|m|m.len()).unwrap_or(0)},
            "config_before":self.config_before,"config_after":config_snapshot(&self.home.join(".config/kah")),
            "operator_claude_files":{"accessed":false,"policy":"outside fixture scope"}})
    }
    pub fn publish(&self, value: &Value) {
        let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
        fs::create_dir_all(&evidence).unwrap();
        save(&evidence.join(format!("{}.json", self.name)), value);
    }
}
pub fn executable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
pub fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
pub fn digest(path: &Path) -> String {
    // Native whole-file hashing avoids unoptimized per-block Rust work on the
    // retained 238 MB source. --zero keeps unusual filenames unescaped.
    let output = Command::new("/usr/bin/sha256sum")
        .args(["--zero", "--"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "TRACK2_DIGEST stage=audit");
    String::from_utf8(output.stdout[..64].to_vec()).unwrap()
}
pub fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TRACK2_GIT stage=fixture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
pub fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn config_snapshot(root: &Path) -> Value {
    let mut entries = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    let files=entries.iter().map(|path| {
        let metadata=fs::symlink_metadata(path).unwrap();
        assert!(metadata.is_file(),"TRACK2_CONFIG_TYPE stage=audit");
        json!({"name":path.file_name(),"mode":metadata.permissions().mode(),"sha256":digest(path)})
    }).collect::<Vec<_>>();
    json!({"mode":fs::metadata(root).unwrap().permissions().mode(),"files":files})
}
