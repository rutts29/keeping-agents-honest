use kah_cli::sandbox_command::Spec;
use kah_sandbox::{Domain, Executor, LaunchOptions, Project};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::PathBuf,
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("kah-command-spec-{}-{label}", std::process::id()));
        fs::create_dir_all(root.join("temp")).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }
    fn spec(&self) -> Spec {
        Spec {
            executor: Executor::new(
                &Project {
                    domain: Domain::Research,
                    root: self.0.clone(),
                    relative_cwd: Default::default(),
                    needs_creation: false,
                },
                LaunchOptions::default(),
                &self.0.join("native"),
                "image",
                42420,
            )
            .unwrap(),
            temporary: Some(self.0.join("temp")),
            service_uid: 42420,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn spec_roundtrip_keeps_native_arguments_and_uses_the_current_directory() {
    let f = Fixture::new("roundtrip");
    let path = f.0.join("spec.json");
    f.spec().save(&path).unwrap();
    let spec = Spec::load(&path).unwrap();
    let command = vec![
        "/bin/echo".into(),
        "a; $(not-a-host-command) ' quoted".into(),
    ];
    let cwd = f.0.join("nested");
    let process = spec.executor.command(&cwd, &command).unwrap();
    let args = process
        .get_args()
        .map(|a| a.to_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(args.ends_with(&command));
    assert!(
        args.windows(2)
            .any(|p| p == ["--workdir", cwd.to_str().unwrap()])
    );
    assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn missing_or_unsafe_specs_fail_with_a_stage_without_exposing_contents() {
    let f = Fixture::new("refusal");
    assert!(
        Spec::load(&f.0.join("absent"))
            .err()
            .unwrap()
            .to_string()
            .contains("SANDBOX_SPEC_READ stage=command")
    );
    fs::write(f.0.join("invalid"), "not JSON PRIVATE_VALUE").unwrap();
    fs::set_permissions(f.0.join("invalid"), fs::Permissions::from_mode(0o600)).unwrap();
    let error = Spec::load(&f.0.join("invalid")).err().unwrap().to_string();
    assert!(error.contains("SANDBOX_SPEC_JSON"));
    assert!(!error.contains("PRIVATE_VALUE"));
    symlink(f.0.join("invalid"), f.0.join("link")).unwrap();
    assert!(
        Spec::load(&f.0.join("link"))
            .err()
            .unwrap()
            .to_string()
            .contains("SANDBOX_SPEC_OWNER")
    );
}

#[test]
fn native_temp_access_does_not_follow_links_or_change_ownership() {
    let f = Fixture::new("temporary");
    let native = f.0.join("temp/native-cwd");
    let private = f.0.join("private");
    fs::write(&native, "cwd").unwrap();
    fs::write(&private, "private").unwrap();
    for path in [&native, &private] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    symlink(&private, f.0.join("temp/link")).unwrap();
    let owner = fs::metadata(&native).unwrap().uid();
    f.spec().command(&f.0, &["/bin/true".into()]).unwrap();
    assert_eq!(fs::metadata(&private).unwrap().mode() & 0o777, 0o600);
    assert_eq!(fs::metadata(&native).unwrap().uid(), owner);
    let acl = std::process::Command::new("/usr/bin/getfacl")
        .args(["-cpn"])
        .arg(&native)
        .output()
        .unwrap();
    assert!(acl.status.success());
    assert!(
        String::from_utf8(acl.stdout)
            .unwrap()
            .contains("user:42420:rw-")
    );
}
