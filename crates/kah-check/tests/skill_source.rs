#[path = "../examples/support/skill_source.rs"]
mod skill_source;
use std::fs;
#[test]
fn a_private_copy_requires_exact_maintained_bytes_and_private_location() {
    let root = std::env::temp_dir().join(format!("kah-skill-source-unit-{}", std::process::id()));
    for dir in ["home/skills/sample", "seeds/sample", "outside"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let private = root.join("home/skills/sample/SKILL.md");
    fs::write(&private, "maintained complete skill\n").unwrap();
    fs::write(
        root.join("seeds/sample/SKILL.md"),
        "maintained complete skill\n",
    )
    .unwrap();
    let seed = root.join("seeds");
    let home = root.join("home");
    assert!(skill_source::matches(&private, "sample", Some(&seed), &home).unwrap());
    fs::write(&private, "changed skill\n").unwrap();
    assert!(!skill_source::matches(&private, "sample", Some(&seed), &home).unwrap());
    let outside = root.join("outside/SKILL.md");
    fs::write(&outside, "maintained complete skill\n").unwrap();
    assert!(!skill_source::matches(&outside, "sample", Some(&seed), &home).unwrap());
    fs::remove_dir_all(root).unwrap();
}
