use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("kah-size-{}-{name}", std::process::id()));
        fs::create_dir_all(path.join("src")).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, lines: usize) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "// fixture\n".repeat(lines)).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn production_limit_includes_nested_crates_and_untracked_files() {
    let fixture = Fixture::new("production");
    fixture.write("crates/example/src/deep/file.rs", 401);
    assert!(
        kah_check::check_tree(&fixture.0)
            .unwrap_err()
            .contains("FILE_TOO_LARGE")
    );
}

#[test]
fn limits_allow_400_production_and_600_test_lines() {
    let fixture = Fixture::new("boundary");
    fixture.write("src/lib.rs", 400);
    fixture.write("src/module/tests.rs", 600);
    fixture.write("tests/journey.rs", 600);
    assert!(kah_check::check_tree(&fixture.0).is_ok());
    fixture.write("tests/journey.rs", 601);
    assert!(kah_check::check_tree(&fixture.0).is_err());
}

#[test]
fn build_output_is_excluded() {
    let fixture = Fixture::new("build");
    fixture.write("target/generated.rs", 1000);
    assert!(kah_check::check_tree(&fixture.0).is_ok());
}

#[test]
fn a_configured_build_directory_with_any_name_is_excluded() {
    let fixture = Fixture::new("custom-build");
    fixture.write("out/debug/generated.rs", 1000);
    assert!(kah_check::check_tree(&fixture.0).is_err());
    let build = fixture.0.join("out");
    assert!(kah_check::check_tree_excluding(&fixture.0, Some(&build)).is_ok());
    fixture.write("src/lib.rs", 401);
    assert!(kah_check::check_tree_excluding(&fixture.0, Some(&build)).is_err());
}

#[test]
fn non_rust_test_files_also_obey_the_test_size_limit() {
    let fixture = Fixture::new("shell-test");
    fixture.write("tests/journey.txt", 601);
    assert!(kah_check::check_tree(&fixture.0).is_err());
}

#[test]
fn shell_entrypoints_must_be_strictly_under_100_lines() {
    let fixture = Fixture::new("shell-limit");
    fixture.write("bin/kah.sh", 99);
    assert!(kah_check::check_tree(&fixture.0).is_ok());
    fixture.write("bin/kah.sh", 100);
    assert!(kah_check::check_tree(&fixture.0).is_err());
}

#[test]
fn extensionless_shell_entrypoints_are_checked() {
    let fixture = Fixture::new("shell-shebang");
    fs::create_dir_all(fixture.0.join(".githooks")).unwrap();
    fs::write(
        fixture.0.join(".githooks/pre-commit"),
        format!("#!/bin/sh\n{}", "# fixture\n".repeat(99)),
    )
    .unwrap();
    assert!(kah_check::check_tree(&fixture.0).is_err());
}
