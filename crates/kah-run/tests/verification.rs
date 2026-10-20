use kah_run::verification;
use kah_sandbox::{Domain, Executor, LaunchOptions, Project};
use std::{path::Path, time::Duration};
#[test]
fn verification_runs_in_the_same_confined_worktree_and_has_a_deadline() {
    let mut executor = Executor::new(
        &Project {
            domain: Domain::Research,
            root: "/run/worktree".into(),
            relative_cwd: Default::default(),
            needs_creation: false,
        },
        LaunchOptions::default(),
        Path::new("/public/codex"),
        "fixture-image",
        42420,
    )
    .unwrap();
    executor.use_nobody();
    let command = verification::command(&executor, "node --test", Duration::from_secs(30)).unwrap();
    let args = command
        .get_args()
        .map(|a| a.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(command.get_program(), "/usr/bin/timeout");
    assert!(args.contains(&"/usr/bin/docker"));
    assert!(args.windows(2).any(|a| a == ["--runtime", "runsc"]));
    assert!(args.windows(2).any(|a| a == ["--network", "none"]));
    assert_eq!(
        &args[args.len() - 3..],
        ["/bin/sh", "-lc", "umask 0002; node --test"]
    );
    assert!(!args.contains(&"exec-server"));
}

/// Needs the image named by assets/sandbox/image-ref in the service daemon.
/// Doctests run binaries from TMPDIR, so /tmp must allow exec.
#[test]
#[ignore = "live gVisor: sandbox image Rust toolchain"]
fn sandbox_image_builds_and_tests_a_rust_crate_offline_as_nobody() {
    let root =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("rust-probe-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "/// ```\n/// assert_eq!(probe::double(2), 4);\n/// ```\npub fn double(n: u32) -> u32 {\n    n * 2\n}\n\n#[test]\nfn doubles() {\n    assert_eq!(double(3), 6);\n}\n",
    )
    .unwrap();
    let uid = kah_run::access::service_uid().unwrap();
    kah_run::access::share_tree(&root, uid, true).unwrap();
    let image = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/sandbox/image-ref"
    ))
    .unwrap();
    let mut executor = Executor::new(
        &Project {
            domain: Domain::Research,
            root: root.clone(),
            relative_cwd: Default::default(),
            needs_creation: false,
        },
        LaunchOptions::default(),
        Path::new("/usr/bin/true"),
        image.trim(),
        uid,
    )
    .unwrap();
    executor.use_nobody();
    let script = "cargo --version && rustfmt --version && cargo clippy --version && cargo fmt --check && cargo test --offline";
    let receipt = verification::execute(
        &executor,
        script,
        &"0".repeat(40),
        Duration::from_secs(300),
        &root.with_extension("log"),
    )
    .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_file(root.with_extension("log")).unwrap();
    assert!(receipt.success, "{}", receipt.output);
    for line in [
        "cargo 1.97.1",
        "clippy 0.1.97",
        "test doubles ... ok",
        "src/lib.rs - double (line 1) ... ok",
    ] {
        assert!(
            receipt.output.contains(line),
            "missing {line}: {}",
            receipt.output
        );
    }
}
