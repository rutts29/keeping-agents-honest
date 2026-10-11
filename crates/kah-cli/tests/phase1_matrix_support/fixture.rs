use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Fixture {
    pub root: PathBuf,
    pub home: PathBuf,
    pub project: PathBuf,
    pub evidence: PathBuf,
    pub name: String,
    pub corpus: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let actual = PathBuf::from(std::env::var_os("HOME").unwrap());
        let name = format!("kah-phase1-matrix-{}", kah_observer::session::now_ms());
        let root = PathBuf::from("/tmp").join(&name);
        let evidence = PathBuf::from(std::env::var_os("KAH_PHASE1_EVIDENCE").unwrap()).join(&name);
        let home = root.join("home");
        let project = home.join("repos/mine/matrix");
        for path in [
            &root,
            &evidence,
            &project,
            &home.join(".local/bin"),
            &home.join(".codex"),
            &home.join(".codex/sessions"),
            &home.join(".codex/archived_sessions"),
        ] {
            kah_core::run_store::ensure_private_directory(path).unwrap();
        }
        native_home(&actual, &home);
        let shell = root.join("shell");
        fs::write(&shell,format!("#!/bin/sh\nexec env HOME={} XDG_STATE_HOME={} KAH_STATE_DIR={} PS1=KAH_PHASE1_READY /bin/bash --noprofile --norc -i\n",crate::pty::quote(home.to_str().unwrap()),crate::pty::quote(home.join(".local/state").to_str().unwrap()),crate::pty::quote(home.join(".local/state/kah").to_str().unwrap()))).unwrap();
        fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.join("herdr.toml"),
            format!(
                "onboarding=false\n[terminal]\ndefault_shell={}\n",
                json!(shell)
            ),
        )
        .unwrap();
        let corpus = PathBuf::from(std::env::var_os("KAH_PHASE1_CORPUS").unwrap());
        let configs = [".codex/config.toml", ".codex/hooks.json"]
            .into_iter()
            .filter_map(|relative| {
                let path = actual.join(relative);
                fs::read(&path).ok().map(|b| {
                    (
                        path.to_string_lossy().to_string(),
                        kah_observer::commit_check::digest(&b),
                    )
                })
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        save(&evidence.join("global-config-before.json"), &json!(configs));
        let fixture = Self {
            root,
            home,
            project,
            evidence,
            name,
            corpus,
        };
        save(
            &fixture.root.join("fixture.json"),
            &serde_json::to_value(&fixture).unwrap(),
        );
        save(
            &fixture.evidence.join("ownership.json"),
            &serde_json::to_value(&fixture).unwrap(),
        );
        fixture
    }
    pub fn read(root: &Path) -> Self {
        serde_json::from_slice(&fs::read(root.join("fixture.json")).unwrap()).unwrap()
    }
}

fn native_home(actual: &Path, home: &Path) {
    let capacity = usage::resources(home);
    let required = fs::metadata(env!("CARGO_BIN_EXE_kah")).unwrap().len();
    assert!(
        capacity["tmpfs_free_bytes"].as_u64().unwrap() >= required,
        "PHASE1_TMPFS_SPACE stage=fixture: actual binary copy does not fit"
    );
    fs::copy(env!("CARGO_BIN_EXE_kah"), home.join(".local/bin/kah")).unwrap();
    fs::set_permissions(
        home.join(".local/bin/kah"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    symlink(
        actual.join(".local/bin/codex"),
        home.join(".local/bin/codex"),
    )
    .unwrap();
    symlink(
        actual.join(".codex/auth.json"),
        home.join(".codex/auth.json"),
    )
    .unwrap();
    fs::write(
        home.join(".codex/config.toml"),
        "model=\"gpt-6.1-sol\"\nmodel_reasoning_effort=\"low\"\nservice_tier=\"default\"\n",
    )
    .unwrap();
}
