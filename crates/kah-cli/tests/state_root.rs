//! Every kah store follows the one state root: an absolute XDG_STATE_HOME moves
//! all of it, and Claude's file tools are denied that resolved root.
use kah_cli::{Arguments, claude_policy, resume};
use std::{fs, path::Path, process::Command};

const CHILD: &str = "KAH_STATE_ROOT_CHILD";

#[test]
fn an_absolute_xdg_state_home_moves_every_store() {
    let base = std::env::temp_dir().join(format!("kah-state-root-{}", std::process::id()));
    let (home, state) = (base.join("home"), base.join("xdg"));
    fs::create_dir_all(home.join("repos/mine/p")).unwrap();
    fs::create_dir_all(&state).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "state_root_child", "--nocapture"])
        .env(CHILD, "1")
        .env("HOME", &home)
        .env("XDG_STATE_HOME", &state)
        .status()
        .unwrap();
    fs::remove_dir_all(&base).unwrap();
    assert!(status.success());
}

/// Runs only as the child above, with its own HOME and XDG_STATE_HOME.
#[test]
fn state_root_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let home = Path::new(&std::env::var_os("HOME").unwrap()).to_path_buf();
    let root = Path::new(&std::env::var_os("XDG_STATE_HOME").unwrap()).join("kah");
    assert_eq!(resume::overlay_root(&home), root.join("codex"));
    let mut args = Arguments::default();
    let rollout = root.join("codex/research-online/hash/sessions/file");
    resume::apply_location(&home, &rollout, &mut args);
    assert!(
        args.options.online,
        "a saved online overlay is not recognised"
    );
    kah_observer::judge::availability::record_error(&home, "JEV_HTTP_TIMEOUT stage=judge_http")
        .unwrap();
    assert!(root.join("jev-service.json").is_file());
    let project = home.join("repos/mine/p").canonicalize().unwrap();
    let policy =
        claude_policy::permissions(&home, &project, &home.join("private"), false, None).unwrap();
    // Private state kept at the default path from before XDG was set stays
    // protected alongside the active root.
    for path in [root.clone(), home.join(".local/state/kah")] {
        for tool in ["Read", "Edit"] {
            let rule = format!("{tool}(/{}/**)", path.display());
            assert!(
                policy["deny"].as_array().unwrap().contains(&rule.into()),
                "{tool} {}",
                path.display()
            );
        }
    }
    assert!(!home.join(".local/state").exists());
}
