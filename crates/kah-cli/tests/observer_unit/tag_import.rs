//! A tagged gate bundle; model/transport-free tag import and gate routing.
use super::*;
use crate::observer_review::state::State;
// One shared builder; each test module includes its own copy.
#[allow(clippy::duplicate_mod)]
#[path = "synthetic_gate.rs"]
mod synthetic;

#[test]
fn a_gate_tag_survives_import_and_planned_checks_select_the_gate_candidate() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
    let worker = format!("tag-replay-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.join("interactive-gates/review-tag-replay");
    kah_core::run_store::ensure_save_directory(&root).unwrap();
    let project = root.join("run-tag-replay/worktree");
    kah_core::run_store::ensure_save_directory(&project).unwrap();
    let state: State = serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":project,"pane":null,"socket":null},
        "observer":{"session":"fixture-observer","pane":null,"name":"fixture","workspace":root,"model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":null,"checkpoint":null,"last_verified_turn":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let home = std::env::temp_dir().join(format!("tag-home-{}", crate::relay_store::nonce()));
    let head = &synthetic::tagged_bundle(&home.join("origin"), &root.join("source.bundle"));
    let candidate = load(&root, &home, &state, head).unwrap();
    let tag = git()
        .arg("-C")
        .arg(&candidate.path)
        .args(["rev-parse", &format!("{}^{{commit}}", synthetic::TAG)])
        .output()
        .unwrap();
    assert!(
        tag.status.success(),
        "{}",
        String::from_utf8_lossy(&tag.stderr)
    );
    assert_eq!(String::from_utf8(tag.stdout).unwrap().trim(), head);
    source_store::save(&store.directory.join("interactive-candidates"), &format!("{head}.json"),
        &json!({"worker":worker,"head":head,"project":project,"candidate":root.join("candidate.json")})).unwrap();
    let mut request: Request = serde_json::from_value(synthetic::request(&worker, head)).unwrap();
    request.trigger = json!({"interactive_gate_verification":{"synthetic_routing_marker":true}});
    let selected = super::super::super::checks::planned_candidate(&state, head, &request).unwrap();
    assert_eq!(
        selected.path, candidate.path,
        "gate plan must not fall back to the tagless run candidate"
    );
    request.trigger = json!({});
    assert!(
        super::super::super::checks::planned_candidate(&state, head, &request).is_err(),
        "normal completion still requires the authentic run candidate"
    );
    assert_eq!(
        load(&root, &home, &state, head).unwrap().head().unwrap(),
        *head
    );
    reject_missing_target(&root, &candidate, head);
    drop(store);
    fs::remove_dir_all(root.parent().unwrap().parent().unwrap()).unwrap();
    fs::remove_dir_all(home).unwrap();
}

fn reject_missing_target(root: &Path, candidate: &Worktree, head: &str) {
    let original = fs::read(root.join("source.bundle")).unwrap();
    let needle = format!("{head} refs/tags/{}", synthetic::TAG);
    let needle = needle.as_bytes();
    let offset = original
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap();
    let mut malformed = original.clone();
    malformed[offset..offset + 40].fill(b'0');
    let bad = root.join("malformed-advertised-tag.bundle");
    fs::write(&bad, malformed).unwrap();
    assert!(
        tags::import(candidate, &bad).is_err(),
        "an advertised target missing from the bundle is not evidence"
    );
    let tag = git()
        .arg("-C")
        .arg(&candidate.path)
        .args(["rev-parse", &format!("{}^{{commit}}", synthetic::TAG)])
        .output()
        .unwrap();
    assert!(tag.status.success());
    assert_eq!(String::from_utf8(tag.stdout).unwrap().trim(), head);
    assert_eq!(
        fs::read(root.join("source.bundle")).unwrap(),
        original,
        "the source bundle remains byte-identical"
    );
}
