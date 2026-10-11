//! Actual external-config mount failure; only non-secret mock login files exist here.
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
};

const LOGINS: &[&str] = &[
    ".credentials.json",
    ".credentials.backup",
    "auth.json",
    "login.json",
];

#[test]
#[ignore = "installed sudo-free rootless gVisor ACL regression; no native agents or Jev"]
fn external_config_assets_are_readable_but_logins_and_root_listing_are_not() {
    let uid = kah_run::access::service_uid().unwrap();
    let root = fixture(uid);
    let home = root.join("worker-home");
    let config = root.join("test-template/.claude");
    assert!(!config.starts_with(&home));
    for name in ["plugins", "skills", "commands"] {
        super::share(&config.join(name), uid).unwrap();
    }
    let executor = executor(&root, uid);
    for name in ["plugins", "skills", "commands"] {
        let result = probe(
            &executor,
            &root,
            &config.join(name),
            "cat /probe/public.txt",
        );
        assert!(
            result.status.success(),
            "asset bind/open: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"PUBLIC_ASSET\n");
    }
    protected(&executor, &root, &config, uid);
    println!(
        "EXTERNAL_CONFIG_ACL_PASS service_uid={uid} assets=3 mock_logins=4 root_listing=denied Jev_calls=0 sudo=false"
    );
    fs::remove_dir_all(root).unwrap();
}

fn protected(executor: &kah_sandbox::Executor, root: &Path, config: &Path, uid: u32) {
    for parent in [config, &root.join("test-template")] {
        assert!(acl(parent).contains(&format!("user:{uid}:--x")));
    }
    for name in LOGINS {
        let login = config.join(name);
        assert_eq!(
            fs::metadata(&login).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!acl(&login).contains(&format!("user:{uid}:")));
        denied(probe(executor, root, &login, "cat /probe >/dev/null"));
    }
    denied(probe(executor, root, config, "ls /probe >/dev/null"));
}

fn fixture(uid: u32) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "kah-external-config-acl-{}",
        crate::relay_store::nonce()
    ));
    fs::create_dir_all(root.join("worker-home")).unwrap();
    fs::create_dir(root.join("project")).unwrap();
    let operator = kah_sandbox::current_uid().unwrap();
    for parent in root.ancestors() {
        if fs::metadata(parent).unwrap().uid() != operator {
            break;
        }
        kah_run::access::traverse(parent, uid).unwrap();
    }
    kah_run::access::share_tree(&root.join("project"), uid, true).unwrap();
    let config = root.join("test-template/.claude");
    fs::create_dir_all(&config).unwrap();
    for parent in [&config, &root.join("test-template")] {
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for name in ["plugins", "skills", "commands"] {
        fs::create_dir(config.join(name)).unwrap();
        fs::write(config.join(name).join("public.txt"), b"PUBLIC_ASSET\n").unwrap();
    }
    for name in LOGINS {
        fs::write(config.join(name), b"NON_SECRET_TEST_SENTINEL\n").unwrap();
        fs::set_permissions(config.join(name), fs::Permissions::from_mode(0o600)).unwrap();
    }
    symlink(
        config.join(".credentials.json"),
        config.join("plugins/login-link"),
    )
    .unwrap();
    fs::write(root.join("codex"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(root.join("codex"), fs::Permissions::from_mode(0o755)).unwrap();
    root
}

fn executor(root: &Path, uid: u32) -> kah_sandbox::Executor {
    assert_eq!(
        fs::metadata(format!("/run/user/{uid}/docker.sock"))
            .unwrap()
            .uid(),
        uid
    );
    let project = kah_sandbox::Project {
        domain: kah_sandbox::Domain::Research,
        root: root.join("project"),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
    let mut executor = kah_sandbox::Executor::new(
        &project,
        Default::default(),
        &root.join("codex"),
        image.trim(),
        uid,
    )
    .unwrap();
    executor.use_nobody();
    executor.insert_options(["--pull".into(), "never".into()]);
    executor
}

fn probe(executor: &kah_sandbox::Executor, root: &Path, source: &Path, command: &str) -> Output {
    executor
        .clone()
        .with_read_only_mounts(&[(source.into(), "/probe".into())])
        .unwrap()
        .command(
            &root.join("project"),
            &["/bin/sh".into(), "-c".into(), command.into()],
        )
        .unwrap()
        .output()
        .unwrap()
}

fn acl(path: &Path) -> String {
    let output = Command::new("/usr/bin/getfacl")
        .args(["-cpn"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let result = String::from_utf8(output.stdout).unwrap();
    println!("ACL {}\n{result}", path.display());
    result
}

fn denied(output: Output) {
    assert!(
        !output.status.success(),
        "service account read a protected fixture path"
    );
    let error = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(
        error.contains("permission denied") || error.contains("eacces"),
        "{error}"
    );
}
