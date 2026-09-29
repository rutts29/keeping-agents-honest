use kah_codex::overlay::{Policy, prepare};
use std::{fs, os::unix::fs::symlink, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("kah-overlay-{}-{name}", std::process::id()));
        fs::create_dir_all(root.join("original")).unwrap();
        fs::write(root.join("original/config.toml"), "model='user-choice'").unwrap();
        Self(root)
    }
    fn prepare(&self) -> Result<(), kah_codex::overlay::OverlayError> {
        prepare(
            &self.0.join("original"),
            &self.0.join("overlay"),
            &Policy {
                restricted: true,
                online: false,
                root: &self.0,
                cwd: &self.0,
            },
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn native_config_writes_do_not_reach_original_but_login_and_history_are_shared() {
    let f = Fixture::new("independent");
    f.prepare().unwrap();
    let overlay = f.0.join("overlay");
    fs::write(overlay.join("config.toml"), "native local setting").unwrap();
    assert_eq!(
        fs::read_to_string(f.0.join("original/config.toml")).unwrap(),
        "model='user-choice'"
    );
    for name in ["auth.json", "sessions", "history.jsonl"] {
        assert_eq!(
            fs::read_link(overlay.join(name)).unwrap(),
            f.0.join("original").join(name)
        );
    }
    assert!(!overlay.join("hooks.json").exists());
    f.prepare().unwrap();
}

#[test]
fn a_previous_config_symlink_is_refused_without_touching_the_global_target() {
    let f = Fixture::new("symlink");
    fs::create_dir(f.0.join("overlay")).unwrap();
    symlink(
        f.0.join("original/config.toml"),
        f.0.join("overlay/config.toml"),
    )
    .unwrap();
    assert_eq!(f.prepare().unwrap_err().code, "OVERLAY_CONFIG_TYPE");
    assert_eq!(
        fs::read_to_string(f.0.join("original/config.toml")).unwrap(),
        "model='user-choice'"
    );
}
