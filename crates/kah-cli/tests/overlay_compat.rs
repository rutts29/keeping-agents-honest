//! An absolute XDG_STATE_HOME moves the state root, but overlays and saved
//! policies made before it was set keep working: a resumed session reuses its
//! saved overlay and an old rollout still carries its online or yolo choice.
use kah_cli::{Arguments, resume, session_policy::SessionPolicy};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

const CHILD: &str = "KAH_OVERLAY_COMPAT_CHILD";
const HASH: &str = "0123456789abcdef";

#[test]
fn old_overlays_survive_a_moved_state_root() {
    let base = std::env::temp_dir().join(format!("kah-overlay-compat-{}", std::process::id()));
    let (home, state) = (base.join("home"), base.join("xdg"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "overlay_compat_child", "--nocapture"])
        .env(CHILD, "1")
        .env("HOME", &home)
        .env("XDG_STATE_HOME", &state)
        .output()
        .unwrap();
    fs::remove_dir_all(&base).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Runs only as the child above, with its own HOME and XDG_STATE_HOME.
#[test]
fn overlay_compat_child() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let home = Path::new(&std::env::var_os("HOME").unwrap()).to_path_buf();
    let active = Path::new(&std::env::var_os("XDG_STATE_HOME").unwrap()).join("kah/codex");
    let former = home.join(".local/state/kah/codex");
    assert_eq!(
        resume::overlay_roots(&home),
        [active.clone(), former.clone()]
    );
    old_rollouts_keep_their_choice(&home, &active, &former);
    overlay_choice(&home, &active, &former);
    saved_policy_keeps_its_overlay(&home, &former);
}

fn choice(home: &Path, rollout: &Path) -> (bool, bool) {
    let mut args = Arguments::default();
    resume::apply_location(home, rollout, &mut args);
    (args.options.online, args.options.yolo)
}

fn old_rollouts_keep_their_choice(home: &Path, active: &Path, former: &Path) {
    for root in [active, former] {
        let rollout = |policy: &str| root.join(policy).join(HASH).join("sessions/file");
        assert_eq!(choice(home, &rollout("research-online")), (true, false));
        assert_eq!(choice(home, &rollout("untrusted-online")), (true, false));
        assert_eq!(choice(home, &rollout("mine-yolo")), (false, true));
        assert_eq!(choice(home, &rollout("research-offline")), (false, false));
    }
    let unrelated = home.join("elsewhere/codex/research-online").join(HASH);
    assert_eq!(choice(home, &unrelated), (false, false));
}

fn overlay_choice(home: &Path, active: &Path, former: &Path) {
    let policy = "research-online";
    let (new, old) = (
        active.join(policy).join(HASH),
        former.join(policy).join(HASH),
    );
    assert_eq!(
        resume::overlay_for(home, policy, HASH, None),
        new,
        "a new overlay"
    );
    fs::create_dir_all(&old).unwrap();
    assert_eq!(
        resume::overlay_for(home, policy, HASH, None),
        old,
        "an existing old one is kept"
    );
    fs::create_dir_all(&new).unwrap();
    assert_eq!(
        resume::overlay_for(home, policy, HASH, None),
        new,
        "the active root first"
    );
    assert_eq!(
        resume::overlay_for(home, policy, HASH, Some(&old)),
        old,
        "the saved binding wins"
    );
    let foreign = home.join("elsewhere").join(policy).join(HASH);
    assert_eq!(
        resume::overlay_for(home, policy, HASH, Some(&foreign)),
        new,
        "an unrelated path is ignored"
    );
}

fn saved_policy_keeps_its_overlay(home: &Path, former: &Path) {
    let session = "00000000-0000-4000-8000-000000000001";
    let project = home.join("repos/research/demo");
    fs::create_dir_all(&project).unwrap();
    let saved = SessionPolicy {
        project: project.clone(),
        policy: "research-online".into(),
        overlay: former.join("research-online").join(HASH),
    };
    let directory =
        Path::new(&std::env::var_os("XDG_STATE_HOME").unwrap()).join("kah/session-policies");
    fs::create_dir_all(&directory).unwrap();
    let file = directory.join(format!("{:x}.json", Sha256::digest(session.as_bytes())));
    fs::write(file, serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut args = Arguments::default();
    let rollout = former
        .join("research-online")
        .join(HASH)
        .join("sessions/file");
    let target = resume::restore_policy(&project, &rollout, home, session, &mut args).unwrap();
    assert_eq!(target, project);
    assert!(args.options.online);
    assert_eq!(args.saved_overlay.as_deref(), Some(saved.overlay.as_path()));
    // The launch for that session picks the same overlay, so the policy guard sees equal bindings.
    let chosen = resume::overlay_for(home, "research-online", HASH, args.saved_overlay.as_deref());
    assert_eq!(chosen, saved.overlay);
}
