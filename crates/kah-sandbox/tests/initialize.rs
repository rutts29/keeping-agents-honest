use kah_sandbox::{Domain, Project};
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn initialize_leaves_guidance_uncommitted_and_never_reinitializes_an_existing_project() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-init-test-{}", std::process::id())));
    fs::create_dir_all(f.0.join("mine")).unwrap();
    let project = Project::select(&f.0, &f.0, Some((Domain::Mine, "demo"))).unwrap();
    project.initialize().unwrap();
    assert!(project.root.join("AGENTS.md").is_file());
    assert_eq!(
        fs::read_link(project.root.join("CLAUDE.md")).unwrap(),
        PathBuf::from("AGENTS.md")
    );
    let output = Command::new("git")
        .arg("-C")
        .arg(&project.root)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("AGENTS.md"));
    assert_eq!(project.initialize().unwrap_err().code, "PROJECT_EXISTS");
    assert!(
        !Command::new("git")
            .arg("-C")
            .arg(&project.root)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn existing(f: &Fixture, domain: Domain, dir: &str, name: &str) -> Project {
    let root = f.0.join(dir).join(name);
    fs::create_dir_all(&root).unwrap();
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );
    Project::select(&f.0, &root, Some((domain, name))).unwrap()
}

fn global_config(f: &Fixture, body: &str) -> PathBuf {
    let path = f.0.join("gitconfig");
    fs::write(&path, body).unwrap();
    path
}

const GLOBAL: &str =
    "[user]\n\tname = Global Me\n\temail = me@example.com\n[credential]\n\thelper = store\n";

#[test]
fn existing_research_repo_without_identity_gets_only_the_global_identity() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-ident-a-{}", std::process::id())));
    let project = existing(&f, Domain::Research, "research", "demo");
    let global = global_config(&f, GLOBAL);
    assert!(project.ensure_identity_from(&global).unwrap());
    assert_eq!(
        git(&project.root, &["config", "--local", "user.name"]),
        "Global Me"
    );
    assert_eq!(
        git(&project.root, &["config", "--local", "user.email"]),
        "me@example.com"
    );
    assert_eq!(
        git(&project.root, &["config", "--local", "credential.helper"]),
        ""
    );
    assert!(!project.ensure_identity_from(&global).unwrap());
}

#[test]
fn existing_local_identity_is_never_overwritten() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-ident-b-{}", std::process::id())));
    let project = existing(&f, Domain::Research, "research", "demo");
    git(&project.root, &["config", "user.name", "Local Me"]);
    git(
        &project.root,
        &["config", "user.email", "local@example.com"],
    );
    let global = global_config(&f, GLOBAL);
    assert!(!project.ensure_identity_from(&global).unwrap());
    assert_eq!(
        git(&project.root, &["config", "--local", "user.name"]),
        "Local Me"
    );
    assert_eq!(
        git(&project.root, &["config", "--local", "user.email"]),
        "local@example.com"
    );
}

#[test]
fn mine_repo_is_untouched() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-ident-c-{}", std::process::id())));
    let project = existing(&f, Domain::Mine, "mine", "demo");
    let global = global_config(&f, GLOBAL);
    assert!(!project.ensure_identity_from(&global).unwrap());
    assert_eq!(git(&project.root, &["config", "--local", "user.name"]), "");
}

#[test]
fn missing_global_identity_changes_nothing_and_does_not_fail() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-ident-d-{}", std::process::id())));
    let project = existing(&f, Domain::Untrusted, "untrusted", "demo");
    let global = global_config(&f, "");
    assert!(!project.ensure_identity_from(&global).unwrap());
    assert_eq!(git(&project.root, &["config", "--local", "user.name"]), "");
}

#[test]
fn observer_workspace_without_a_repository_launches_with_a_global_identity() {
    // OBS-ACCEPT lab-cp1 (real HOME): the observer workspace is a plain
    // directory; writing identity there failed every Claude observer launch.
    let f = Fixture(std::env::temp_dir().join(format!("kah-ident-ws-{}", std::process::id())));
    let root = f.0.join("research").join("observer-workspace");
    fs::create_dir_all(&root).unwrap();
    let workspace =
        Project::select(&f.0, &root, Some((Domain::Research, "observer-workspace"))).unwrap();
    let global = global_config(&f, GLOBAL);
    assert!(!workspace.ensure_identity_from(&global).unwrap());
    assert!(!root.join(".git").exists());
}

#[test]
fn a_directory_inside_another_repository_never_edits_that_repository() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-ident-nest-{}", std::process::id())));
    let parent = existing(&f, Domain::Research, "research", "parent");
    let root = parent.root.join("nested");
    fs::create_dir_all(&root).unwrap();
    let nested = Project {
        domain: Domain::Research,
        root,
        relative_cwd: PathBuf::new(),
        needs_creation: false,
    };
    let global = global_config(&f, GLOBAL);
    assert!(!nested.ensure_identity_from(&global).unwrap());
    assert_eq!(git(&parent.root, &["config", "--local", "user.name"]), "");
}
