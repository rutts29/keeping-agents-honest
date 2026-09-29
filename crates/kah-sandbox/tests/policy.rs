use kah_sandbox::{Domain, LaunchOptions, Project};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("kah-policy-{}-{name}", std::process::id()));
        for domain in ["mine", "research", "untrusted"] {
            fs::create_dir_all(root.join(domain).join("demo/subdir")).unwrap();
        }
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn nested_directory_selects_one_project_and_preserves_relative_cwd() {
    let fixture = Fixture::new("nested");
    for domain in [Domain::Mine, Domain::Research, Domain::Untrusted] {
        let cwd = fixture.0.join(domain.as_str()).join("demo/subdir");
        let project = Project::select(&fixture.0, &cwd, None).unwrap();
        assert_eq!(project.domain, domain);
        assert_eq!(project.root, fixture.0.join(domain.as_str()).join("demo"));
        assert_eq!(project.relative_cwd, PathBuf::from("subdir"));
    }
}

#[test]
fn whole_domain_and_outside_directories_are_rejected() {
    let fixture = Fixture::new("boundaries");
    assert_eq!(
        Project::select(&fixture.0, &fixture.0.join("research"), None)
            .unwrap_err()
            .code,
        "PROJECT_REQUIRED"
    );
    assert_eq!(
        Project::select(&fixture.0, &std::env::temp_dir(), None)
            .unwrap_err()
            .code,
        "DOMAIN_REQUIRED"
    );
}

#[cfg(unix)]
#[test]
fn a_research_symlink_cannot_upgrade_to_mine_or_escape_a_project() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new("symlink");
    symlink(
        fixture.0.join("mine/demo"),
        fixture.0.join("research/escape"),
    )
    .unwrap();
    let error = Project::select(&fixture.0, &fixture.0.join("research/escape"), None).unwrap_err();
    assert_eq!(error.code, "PROJECT_ESCAPE");
    symlink(
        fixture.0.join("research/demo"),
        fixture.0.join("research/alias"),
    )
    .unwrap();
    assert!(Project::select(&fixture.0, &fixture.0.join("research/alias"), None).is_ok());
}

#[cfg(unix)]
#[test]
fn a_domain_root_alias_cannot_change_the_trust_label() {
    let fixture = Fixture::new("domain-alias");
    fs::remove_dir_all(fixture.0.join("research")).unwrap();
    std::os::unix::fs::symlink(fixture.0.join("mine"), fixture.0.join("research")).unwrap();
    assert_eq!(
        Project::select(&fixture.0, &fixture.0.join("research/demo"), None)
            .unwrap_err()
            .code,
        "DOMAIN_ALIAS"
    );
}

#[test]
fn network_and_yolo_require_their_declared_domain() {
    let online = LaunchOptions {
        online: true,
        yolo: false,
    };
    let yolo = LaunchOptions {
        online: false,
        yolo: true,
    };
    assert!(online.validate(Domain::Research).is_ok());
    assert!(online.validate(Domain::Mine).is_err());
    assert!(online.validate(Domain::Untrusted).is_err());
    assert!(yolo.validate(Domain::Mine).is_ok());
    assert!(yolo.validate(Domain::Research).is_err());
    assert!(yolo.validate(Domain::Untrusted).is_err());
}

#[test]
fn new_named_projects_cannot_contain_path_syntax() {
    let fixture = Fixture::new("names");
    for name in ["../escape", ".", "..", "two/levels", "", "-option"] {
        assert!(Project::select(&fixture.0, &fixture.0, Some((Domain::Mine, name))).is_err());
    }
    let project = Project::select(
        &fixture.0,
        &fixture.0,
        Some((Domain::Research, "new-project")),
    )
    .unwrap();
    assert!(project.needs_creation);
    assert_eq!(project.root, fixture.0.join("research/new-project"));
}
