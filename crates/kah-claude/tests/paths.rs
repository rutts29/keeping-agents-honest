use kah_claude::paths::project_key;
use std::path::Path;
#[test]
fn project_key_matches_installed_292_utf16_and_long_path_hash() {
    assert_eq!(
        project_key(Path::new(
            "/mnt/kah-fixtures/project.name space+[é]😀"
        )),
        "-mnt-HC-Volume-106758648-project-name-space------"
    );
    let long = format!("/mnt/{}", "project_".repeat(30));
    let expected = "-mnt-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-project-pro-jvdrk1";
    assert_eq!(project_key(Path::new(&long)), expected);
    assert_eq!(project_key(Path::new("/Project_A.B")), "-Project-A-B");
}
