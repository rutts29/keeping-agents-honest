use kah_sandbox::global_setup_mounts;
use std::{fs, path::PathBuf};

#[test]
fn global_setup_uses_live_sources_and_resolves_installed_skill_aliases_only() {
    let home = std::env::temp_dir().join(format!("kah-global-setup-{}", std::process::id()));
    fs::create_dir_all(home.join(".agents/skills")).unwrap();
    fs::create_dir_all(home.join("packages/example-skill")).unwrap();
    fs::create_dir_all(home.join(".codex/plugins")).unwrap();
    fs::create_dir_all(home.join(".claude/skills")).unwrap();
    fs::write(home.join(".codex/AGENTS.md"), "global guidance").unwrap();
    fs::write(home.join(".codex/auth.json"), "must not mount").unwrap();
    let skill = home.join("packages/example-skill");
    fs::write(skill.join("SKILL.md"), "first version").unwrap();
    std::os::unix::fs::symlink(&skill, home.join(".agents/skills/example")).unwrap();
    let mounts = global_setup_mounts(&home).unwrap();
    for path in [
        home.join(".agents/skills"),
        home.join(".codex/plugins"),
        home.join(".claude/skills"),
        home.join(".codex/AGENTS.md"),
        skill.clone(),
    ] {
        assert!(
            mounts.contains(&(path.clone(), path)),
            "installed setup omitted"
        );
    }
    assert!(!mounts.iter().any(|(source, _)| source == &home
        || source == &home.join(".codex")
        || source.ends_with("auth.json")));
    fs::write(skill.join("SKILL.md"), "updated version").unwrap();
    let source = &mounts
        .iter()
        .find(|(_, target)| target == &skill)
        .unwrap()
        .0;
    assert_eq!(
        fs::read_to_string(source.join("SKILL.md")).unwrap(),
        "updated version"
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn absent_global_setup_is_not_invented() {
    assert!(
        global_setup_mounts(&PathBuf::from("/nonexistent-kah-global-setup"))
            .unwrap()
            .is_empty()
    );
}
