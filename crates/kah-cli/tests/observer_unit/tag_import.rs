//! Exact retained VF15 bundle; model/transport-free tag import and gate routing.
use super::*;
use crate::observer_review::state::State;

#[test]
fn retained_gate_tag_survives_import_and_planned_checks_select_the_gate_candidate() {
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
    fs::write(
        root.join("source.bundle"),
        include_bytes!("../../../../docs/gates/evidence/phase3-tag-import/retained-source.bundle"),
    )
    .unwrap();
    let head = "8dfddba6469f55882370a4c5432c3b605b4857ce";
    let candidate = load(&root, &state, head).unwrap();
    let tag = git()
        .arg("-C")
        .arg(&candidate.path)
        .args(["rev-parse", "gate-pipeline-ready^{commit}"])
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
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-verifier-replay/retained-input.json"
    ))
    .unwrap();
    let mut request: Request = serde_json::from_value(value["request"].clone()).unwrap();
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
    assert_eq!(load(&root, &state, head).unwrap().head().unwrap(), head);
    reject_missing_target(&root, &candidate);
    drop(store);
    fs::remove_dir_all(root.parent().unwrap().parent().unwrap()).unwrap();
}

fn reject_missing_target(root: &Path, candidate: &Worktree) {
    let original = fs::read(root.join("source.bundle")).unwrap();
    let needle = b"8dfddba6469f55882370a4c5432c3b605b4857ce refs/tags/gate-pipeline-ready";
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
        .args(["rev-parse", "gate-pipeline-ready^{commit}"])
        .output()
        .unwrap();
    assert!(tag.status.success());
    assert_eq!(
        String::from_utf8(tag.stdout).unwrap().trim(),
        "8dfddba6469f55882370a4c5432c3b605b4857ce"
    );
    assert_eq!(
        fs::read(root.join("source.bundle")).unwrap(),
        original,
        "retained source remains byte-identical"
    );
}
