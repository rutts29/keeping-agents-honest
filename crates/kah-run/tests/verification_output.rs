//! A verification receipt carries its output only up to the limit.
use kah_run::verification::read_output;
use std::fs;

fn log(bytes: &[u8]) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "kah-output-test-{}-{}",
        std::process::id(),
        bytes.len()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("attempt-1.log");
    fs::write(&path, bytes).unwrap();
    (dir, path)
}

#[test]
fn output_up_to_the_limit_is_kept_whole() {
    let (dir, path) = log(&[b'x'; 32]);
    assert_eq!(read_output(&path, 32).unwrap(), [b'x'; 32]);
    assert!(read_output(&path, 33).is_ok());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn one_byte_more_is_refused_with_the_log_path() {
    let (dir, path) = log(&[b'x'; 33]);
    let error = read_output(&path, 32).unwrap_err();
    assert!(
        error.starts_with("RUN_TEST_OUTPUT_LIMIT stage=verification"),
        "{error}"
    );
    assert!(error.contains(&path.display().to_string()), "{error}");
    assert!(path.exists(), "the log stays for the operator");
    fs::remove_dir_all(dir).unwrap();
}
