use kah_run::child;
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn cancellation_reaps_the_owned_shell_and_terminates_its_descendant() {
    let file = std::env::temp_dir().join(format!("kah-child-test-{}", std::process::id()));
    let mut shell = Command::new("/bin/sh");
    shell
        .args([
            "-c",
            "sleep 20 & printf '%s' \"$!\" > \"$1\"; wait",
            "fixture",
        ])
        .arg(&file);
    let mut command = child::parent_bound(&shell, "TERM");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let start = Instant::now();
    assert!(
        child::wait(&mut command, || Ok(
            start.elapsed() >= Duration::from_millis(300)
        ))
        .unwrap()
        .is_none()
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    let pid = fs::read_to_string(&file).unwrap();
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"));
    assert!(stat.is_err() || stat.unwrap().rsplit_once(") ").unwrap().1.starts_with('Z'));
    fs::remove_file(file).unwrap();
    assert!(
        !child::wait(&mut Command::new("/bin/false"), || Ok(false))
            .unwrap()
            .unwrap()
            .success()
    );
}
