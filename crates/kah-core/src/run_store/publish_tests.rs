use super::*;

fn staged_run(root: &Path) -> (PathBuf, PathBuf) {
    let staging = root.join(".creating-test");
    fs::create_dir(&staging).expect("create staging");
    fs::write(staging.join("projection.json"), "built").expect("write built run");
    (staging, root.join("run_01"))
}

fn root(case: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kah-publish-{case}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir(&root).expect("create test root");
    root
}

#[test]
fn publishes_a_built_run_when_the_name_is_free() {
    let root = root("free");
    let (staging, path) = staged_run(&root);
    publish_run_directory(&staging, &path).expect("publish");
    assert!(path.join("projection.json").is_file());
    assert!(!staging.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_empty_directory_created_after_the_check_is_not_replaced() {
    let root = root("empty-after-check");
    let (staging, path) = staged_run(&root);
    let error = publish_after_check(&staging, &path, || fs::create_dir(&path).unwrap())
        .expect_err("destination appeared after the check");
    assert_eq!(error, RunStoreError::RunAlreadyExists);
    assert_eq!(
        fs::read_dir(&path).unwrap().count(),
        0,
        "destination untouched"
    );
    assert!(staging.join("projection.json").is_file(), "built run kept");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_symlink_created_after_the_check_is_not_replaced() {
    let root = root("symlink-after-check");
    let (staging, path) = staged_run(&root);
    let target = root.join("elsewhere");
    fs::create_dir(&target).unwrap();
    let error = publish_after_check(&staging, &path, || {
        std::os::unix::fs::symlink(&target, &path).unwrap();
    })
    .expect_err("destination appeared after the check");
    assert_eq!(error, RunStoreError::RunAlreadyExists);
    assert_eq!(fs::read_link(&path).unwrap(), target);
    assert!(staging.join("projection.json").is_file());
    fs::remove_dir_all(root).unwrap();
}
