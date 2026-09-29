use kah_sandbox::{Domain, Project};
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn initialize_commits_guidance_and_never_reinitializes_an_existing_project() {
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
    assert!(output.stdout.is_empty());
    assert_eq!(project.initialize().unwrap_err().code, "PROJECT_EXISTS");
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&project.root)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
