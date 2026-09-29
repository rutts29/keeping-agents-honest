use kah_sandbox::publish_skills;
use std::{fs, os::unix::fs::symlink};

#[test]
fn snapshots_publish_only_skill_trees_and_reject_nested_links() {
    let root = std::env::temp_dir().join(format!("kah-skill-test-{}", std::process::id()));
    let home = root.join("home");
    let source = home.join(".agents/skills/example");
    fs::create_dir_all(&source).unwrap();
    fs::write(home.join("secret"), "PRIVATE").unwrap();
    fs::write(source.join("SKILL.md"), "PUBLIC SKILL").unwrap();
    let mounts = publish_skills(&home, &root.join("cache")).unwrap();
    assert_eq!(mounts.len(), 1);
    assert_eq!(
        fs::read_to_string(mounts[0].0.join("SKILL.md")).unwrap(),
        "PUBLIC SKILL"
    );
    assert!(!mounts[0].0.join("secret").exists());
    symlink(home.join("secret"), source.join("escape")).unwrap();
    assert!(
        publish_skills(&home, &root.join("cache"))
            .unwrap_err()
            .to_string()
            .contains("SKILL_FILE_TYPE")
    );
    fs::remove_dir_all(root).unwrap();
}
