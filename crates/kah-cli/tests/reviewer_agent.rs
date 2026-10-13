use kah_cli::reviewer_agent::{DEFAULT, ENV, resolve, reviewer_agent};
use std::process::Command;

#[test]
fn default_is_kah_reviewer() {
    assert_eq!(resolve(None), Ok("kah-reviewer"));
    assert_eq!(resolve(Some("")), Ok(DEFAULT));
}

#[test]
fn set_value_is_used() {
    assert_eq!(resolve(Some("rev_2-a")), Ok("rev_2-a"));
    assert_eq!(resolve(Some(&"a".repeat(32))), Ok("a".repeat(32).as_str()));
}

#[test]
fn invalid_value_is_rejected() {
    for bad in [
        "a b",
        "a/b",
        "é",
        "x;y",
        "Abc",
        "1abc",
        "-abc",
        "_abc",
        "aBc",
        &"a".repeat(33),
    ] {
        assert_eq!(resolve(Some(bad)), Err(bad));
    }
}

#[test]
fn child_probe() {
    if std::env::var_os("KAH_PROBE").is_some() {
        println!("AGENT={}", reviewer_agent());
    }
}

fn probe(value: &str) -> (String, String) {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_probe", "--nocapture"])
        .env("KAH_PROBE", "1")
        .env(ENV, value)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn spawned_process_sees_value() {
    let (out, err) = probe("custom-reviewer");
    assert!(out.contains("AGENT=custom-reviewer"), "{out}");
    assert!(err.is_empty(), "{err}");
}

#[test]
fn invalid_value_falls_back_with_one_warning() {
    let (out, err) = probe("bad name");
    assert!(out.contains("AGENT=kah-reviewer"), "{out}");
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("KAH_REVIEWER_AGENT"), "{err}");
}
