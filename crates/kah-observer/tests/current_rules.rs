use kah_observer::current_rules::standing_sources;
use std::{fs, os::unix::fs::symlink};
#[test]
fn standing_aliases_preserve_file_provenance_and_referenced_plan_changes_invalidate_input() {
    let root = std::env::temp_dir().join(format!(
        "kah-standing-{}-{}",
        std::process::id(),
        kah_observer::session::now_ms()
    ));
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("AGENTS.md"), "Use docs/PLAN.md. Never push.").unwrap();
    symlink("AGENTS.md", root.join("CLAUDE.md")).unwrap();
    fs::write(root.join("docs/PLAN.md"), "Current rules: no headless.").unwrap();
    let before = standing_sources(&root).unwrap();
    let agents = before
        .iter()
        .find(|s| s.origin["path"] == root.join("AGENTS.md").to_string_lossy().as_ref())
        .unwrap();
    assert_eq!(agents.origin["kind"], "standing_file");
    assert_eq!(
        agents.origin["provenance"]["instruction_paths"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let old = before
        .iter()
        .find(|s| s.id.ends_with("docs/PLAN.md"))
        .unwrap();
    fs::write(
        root.join("docs/PLAN.md"),
        "Current rules: no headless; retain every failure.",
    )
    .unwrap();
    let after = standing_sources(&root).unwrap();
    let new = after.iter().find(|s| s.id == old.id).unwrap();
    assert_ne!(old.content_digest, new.content_digest);
    assert_eq!(old.text, "Current rules: no headless.");
    fs::remove_dir_all(root).unwrap();
}
