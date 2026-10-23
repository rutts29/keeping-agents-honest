#[path = "helper_binary.rs"]
mod helper_binary;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};
/// The runner's native long-session inventory (see track2_gate.rs long_session).
const LONG_SESSION_INVENTORY: &str = "KAH_TRACK2_LONG_SESSION_INVENTORY";
pub struct Fixture {
    pub root: PathBuf,
    pub home: PathBuf,
    pub name: String,
    pub dry: bool,
    protected: Vec<(PathBuf, String)>,
    logins: Vec<(PathBuf, PathBuf)>,
    config_before: Value,
    operator_config: PathBuf,
    operator_claude: PathBuf,
    claude_before: Value,
    /// The Claude test-login config, resolved once and only for modes that use Claude.
    pub claude_config: Option<PathBuf>,
}
impl Fixture {
    pub fn new(mode: &str) -> Self {
        let actual = PathBuf::from(std::env::var_os("HOME").unwrap());
        let name = format!("kah-gate-track2-{}", kah_observer::session::now_ms());
        let root = fixture_base().join(&name);
        assert!(!root.exists(), "TRACK2_EXISTING_FIXTURE stage=prepare");
        kah_core::run_store::ensure_private_directory(&root).unwrap();
        let home = prepare_home(&root, mode);
        let mut result = Self {
            root,
            home,
            name,
            dry: std::env::var_os("KAH_TRACK2_DRY_RUN").is_some(),
            protected: vec![],
            logins: vec![],
            config_before: config_snapshot(&actual.join(".config/kah")),
            operator_config: actual.join(".config/kah"),
            operator_claude: actual.join(".claude"),
            claude_before: super::claude_metadata::snapshot(&actual.join(".claude")),
            claude_config: super::claude_login::requires_claude(mode)
                .then(super::claude_login::config_dir),
        };
        save(
            &result.root.join("config-before.json"),
            &result.config_before,
        );
        result.native_setup(&actual);
        result.sources(&actual, mode);
        result.seed(mode);
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
        symlink(
            actual.join(".local/bin/codex"),
            self.home.join(".local/bin/codex"),
        )
        .unwrap();
        if let Some(config) = &self.claude_config {
            super::claude_invocation::install(
                &self.home,
                &actual.join(".local/bin/claude"),
                config,
            );
        }
        let binary = std::env::var_os("KAH_TRACK2_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_kah")));
        helper_binary::install(&binary, &self.home.join(".local/bin/kah"));
        let target = actual.join(".codex/auth.json");
        let link = self.home.join(".codex/auth.json");
        symlink(&target, &link).unwrap();
        self.logins.push((link, target));
        super::jev_key::install(actual, &self.home);
        for relative in [".codex/config.toml", ".local/bin/kah"] {
            let path = actual.join(relative);
            if path.is_file() {
                self.protected.push((path.clone(), digest(&path)));
            }
        }
        fs::write(
            self.home.join(".codex/config.toml"),
            "model=\"gpt-6.1-sol\"\nmodel_reasoning_effort=\"low\"\nservice_tier=\"fast\"\n",
        )
        .unwrap();
        save(
            &self.root.join("binary.json"),
            &json!({"source":binary,"sha256":digest(&binary)}),
        );
    }
    fn sources(&mut self, actual: &Path, mode: &str) {
        if mode != "long" {
            return;
        }
        // The runner supplies the native sessions; none ship with the tests.
        let inventory = std::env::var_os(LONG_SESSION_INVENTORY).unwrap_or_else(|| {
            panic!("TRACK2_LONG_SESSION_INVENTORY stage=prepare: set {LONG_SESSION_INVENTORY}")
        });
        let inventory: Value = serde_json::from_slice(&fs::read(inventory).unwrap()).unwrap();
        let mut rows = vec![];
        for row in inventory["inventory"].as_array().unwrap() {
            if row["provider"] == "claude"
                && std::env::var("KAH_TRACK2_DRY_PROVIDER").as_deref() == Ok("codex")
            {
                continue;
            }
            let source = PathBuf::from(row["path"].as_str().unwrap());
            assert!(
                source.starts_with(if row["provider"] == "claude" {
                    self.claude_config().join("projects")
                } else {
                    actual.to_path_buf()
                }),
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
                self.claude_config()
                    .join("projects")
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
    fn seed(&self, mode: &str) {
        let repo = self.home.join("repos/mine/source");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "--quiet"]);
        fs::write(repo.join("PLAN.md"),"# Fixture\nPublic marker must remain BLUE. Phase 1 exposes marker and records absent serializer as a genuine Phase 2 dependency. Phase 2 creates serializer, closes dependency and proves integration. Phase 3 documents the earlier marker finding and its repair. No pushes, dependencies, subagents or changes outside this fixture.\n").unwrap();
        if mode == "native5e" {
            fs::write(repo.join("PLAN.md"), "# Native 5e exit fixture\nThe fixture user requests exactly one native Claude Sonnet 5.5 / low subagent. It writes child.txt containing CHILD_OK. The parent waits for completion and replies CHILD_DONE. Work only in this private worktree; no pushes, installs or account changes.\n").unwrap();
        }
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
        if super::claude_login::requires_claude(mode) {
            super::trust::preseed_claude(&self.home, &self.home.join("repos/mine/fixture-claude"));
        }
    }
    fn claude_config(&self) -> &Path {
        self.claude_config
            .as_deref()
            .expect("TRACK2_CLAUDE_CONFIG stage=sources: this mode has no Claude test login")
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
            .map(|(link, target)| json!({"path":link,"target":target,"symlink_unchanged":
                fs::symlink_metadata(link).is_ok_and(|m|m.file_type().is_symlink()) && fs::read_link(link).is_ok_and(|p|p==*target)}))
            .collect::<Vec<_>>();
        let processes = crate::processes::list()
            .into_iter()
            .filter(|p| p.cwd.starts_with(self.root.to_str().unwrap()))
            .map(|p| json!({"pid":p.pid,"cwd":p.cwd}))
            .collect::<Vec<_>>();
        let claude = self.claude_audit();
        let key: Value =
            serde_json::from_slice(&fs::read(self.root.join("jev-key-cleanup.json")).unwrap())
                .unwrap();
        let passed = processes.is_empty()
            && claude["passed"] == true
            && super::proof_inputs::audit(&self.root)["passed"] == true
            && protected.iter().all(|r| r["unchanged"] == true)
            && logins.iter().all(|r| r["symlink_unchanged"] == true)
            && key["passed"] == true
            && key["removed"] == true
            && fs::symlink_metadata(self.home.join(".config/kah/typesafe.key")).is_err()
            && config_snapshot(&self.operator_config) == self.config_before;
        json!({"passed":passed,"processes":processes,"protected":protected,"login_links":logins,
            "historical_replay_source":super::proof_inputs::audit(&self.root),
            "recorder":{"path":self.root.join("terminal.log"),"bytes":fs::metadata(self.root.join("terminal.log")).map(|m|m.len()).unwrap_or(0)},
            "config_before":self.config_before,"config_after":config_snapshot(&self.operator_config),
            "jev_key_cleanup":key,
            "claude_login":claude})
    }
    pub fn publish(&self, value: &Value) {
        let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
        fs::create_dir_all(&evidence).unwrap();
        save(&evidence.join(format!("{}.json", self.name)), value);
    }
    fn claude_audit(&self) -> Value {
        let path = self.root.join("claude-trust-entries.json");
        let trust: Value = if path.exists() {
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
        } else {
            json!({})
        };
        let projects = trust
            .as_object()
            .unwrap()
            .keys()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let sessions = self.root.join("sessions.json");
        let markers = if sessions.exists() {
            serde_json::from_slice(&fs::read(sessions).unwrap()).unwrap()
        } else {
            vec![]
        };
        super::claude_metadata::audit(
            std::slice::from_ref(&self.root),
            &self.operator_claude,
            &self.claude_before,
            &markers,
            &projects,
        )
    }
}
/// The private `track2` directory below `KAH_TEST_FIXTURE_BASE` (or a
/// directory under the system temporary one). The configured base must be
/// owned by the operator; it is created private when absent. Shared or
/// unowned temporary parents are never modified.
fn fixture_base() -> PathBuf {
    let configured = std::env::var_os("KAH_TEST_FIXTURE_BASE")
        .map_or_else(|| std::env::temp_dir().join("kah-fixtures"), PathBuf::from);
    if !configured.exists() {
        kah_core::run_store::ensure_private_directory(&configured).unwrap();
    }
    let configured = configured.canonicalize().unwrap();
    assert_eq!(
        fs::metadata(&configured).unwrap().uid(),
        kah_sandbox::current_uid().unwrap(),
        "TRACK2_FIXTURE_BASE_OWNER stage=prepare"
    );
    let base = configured.join("track2");
    kah_core::run_store::ensure_private_directory(&base).unwrap();
    base
}
/// The directories from `root` upward that the sandbox account cannot
/// traverse, up to the first one the operator does not own. Those above it
/// are shared and never modified; one that blocks traversal fails the setup.
fn private_ancestors(root: &Path, operator: u32) -> Vec<PathBuf> {
    let mut owned = true;
    let mut private = vec![];
    for path in root.ancestors() {
        let metadata = fs::metadata(path).unwrap();
        let traversable = metadata.mode() & 0o001 != 0;
        owned &= metadata.uid() == operator;
        if owned && !traversable {
            private.push(path.to_path_buf());
        }
        assert!(
            owned || traversable,
            "TRACK2_FIXTURE_ANCESTOR stage=prepare: {} blocks the sandbox account and is not the operator's",
            path.display()
        );
    }
    private
}
fn prepare_home(root: &Path, mode: &str) -> PathBuf {
    // Native Mine runs as the operator; only sandbox fixtures need
    // service-UID traversal.
    if mode != "native5e" {
        let uid = kah_run::access::service_uid().unwrap();
        for parent in private_ancestors(root, kah_sandbox::current_uid().unwrap()) {
            kah_run::access::traverse(&parent, uid).unwrap();
        }
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
    assert!(
        !home.join(".cache/kah-observers").exists(),
        "TRACK2_EXISTING_OBSERVER stage=prepare"
    );
    save(
        &root.join("observer-before.json"),
        &json!({"base":home.join(".cache/kah-observers"),"existed":false}),
    );
    home
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
#[path = "config_snapshot.rs"]
mod config_inventory;
fn config_snapshot(root: &Path) -> Value {
    config_inventory::snapshot(root)
}

#[cfg(test)]
#[path = "config_snapshot_tests.rs"]
mod config_snapshot_tests;
#[cfg(test)]
#[path = "fixture_tests.rs"]
mod fixture_tests;
