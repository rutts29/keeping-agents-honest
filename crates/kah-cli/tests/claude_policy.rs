use kah_cli::claude_policy::permissions;
use std::fs;

#[test]
fn native_permissions_keep_global_skills_readable_and_private_files_denied() {
    let home = std::env::temp_dir().join(format!("kah-claude-policy-{}", std::process::id()));
    for path in [
        ".claude/skills",
        ".codex/plugins",
        "repos/research/current",
        "repos/research/other",
    ] {
        fs::create_dir_all(home.join(path)).unwrap();
    }
    fs::write(home.join(".claude/.credentials.json"), "private").unwrap();
    fs::write(home.join(".codex/auth.json"), "private").unwrap();
    let project = home.join("repos/research/current");
    let policy = permissions(&home, &project, &home.join("private-runtime"), false).unwrap();
    let deny = policy["deny"].as_array().unwrap();
    for path in [
        home.join(".claude/.credentials.json"),
        home.join(".codex/auth.json"),
        home.join("repos/research/other"),
    ] {
        assert!(deny.contains(&format!("Read(/{})", path.display()).into()));
    }
    assert!(!deny.contains(&format!("Read(/{}/**)", home.join(".claude/skills").display()).into()));
    assert!(!deny.contains(&format!("Read(/{}/**)", project.display()).into()));
    assert!(deny.contains(&"WebFetch".into()));
    assert!(!deny.contains(&"WebSearch".into()));
    assert!(
        policy["allow"]
            .as_array()
            .unwrap()
            .contains(&format!("Read(/{}/**)", home.join(".claude/skills").display()).into())
    );
    assert!(
        !permissions(&home, &project, &home.join("private-runtime"), true).unwrap()["deny"]
            .as_array()
            .unwrap()
            .contains(&"WebFetch".into())
    );
    fs::remove_dir_all(home).unwrap();
}
