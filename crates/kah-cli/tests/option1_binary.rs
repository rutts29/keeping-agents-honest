//! Authored binary shape; no native agent, provider request or live-store edit.
use kah_cli::observer_review::{option1::input, source_store};
use kah_core::intent_history::{GoalRevision, History, SourceProvider, SourceRef, SourceRole};
use kah_observer::rule_source::Resolved;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn binary_descriptor_is_native_evidence_and_the_whole_blob_stays_on_demand() {
    let root = std::env::temp_dir().join(format!("binary-review-{}", std::process::id()));
    let private = root.join("private");
    let workspace = root.join("observer");
    let view = workspace.join("inputs/current");
    fs::create_dir_all(private.join("sources")).unwrap();
    source_store::initialize(&private, &workspace).unwrap();
    fs::create_dir_all(&view).unwrap();
    let bytes: &[u8] = &[0x00, 0xff, 0xfe, 0x80, 0x42, 0x00];
    let digest = format!("{:x}", Sha256::digest(bytes));
    fs::write(private.join("sources").join(&digest), bytes).unwrap();
    let reference = json!({"$kah_source":digest,"encoding":"binary"});
    let descriptor = json!({"encoding":"binary","path":"evidence.bundle","size":bytes.len(),
        "sha256":digest,"git_blob":"a".repeat(40),"source":reference});
    let context = json!({"head":"b".repeat(40),"touched_files":[{"path":"evidence.bundle","deleted":false,
        "blob":"a".repeat(40),"patch":"GIT binary patch\n","content":descriptor}]});
    let input = local_input(&context);
    let metadata: Value = serde_json::from_str(&input.files["evidence.bundle"].text).unwrap();
    assert_eq!(metadata, descriptor);
    assert!(
        metadata.get("bytes").is_none(),
        "blob bytes never inline into the source text"
    );
    assert_eq!(
        source_store::expand(reference.clone(), &private.join("sources")).unwrap(),
        reference
    );
    source_store::save(&view, "record.json", &serde_json::to_value(&input).unwrap()).unwrap();
    fs::write(
        view.join("context-input.json"),
        json!({"sources":[],"full_request":view.join("record.json")}).to_string(),
    )
    .unwrap();
    source_store::share(&private, &view).unwrap();
    assert_eq!(fs::read(view.join("sources").join(&digest)).unwrap(), bytes);
    assert_eq!(
        input.carried["binary_sources"]["evidence.bundle"],
        descriptor
    );
    assert!(
        kah_cli::observer_review::option1::requests::build(&input, None)
            .err()
            .expect("binary must not reach literal Jev")
            .to_string()
            .starts_with("COMMIT_BINARY_NATIVE_REQUIRED")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn publishing_git_bytes_keeps_the_binary_patch_out_of_the_native_input() {
    use kah_cli::observer_review::{
        option1::binary,
        state::{State, Store},
    };
    std::env::var_os("XDG_STATE_HOME").expect("private checks required");
    let worker = format!("binary-publish-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("observer");
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},
        "observer":{"session":"same-observer","pane":null,"name":"unit","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let bytes = vec![0xff; 200000];
    let patch = "GIT binary patch\ncomplete encoded payload\n";
    let mut context = json!({"head":"a".repeat(40),"touched_files":[{"path":"whole.bin","blob":"b".repeat(40),
        "deleted":false,"patch":patch,"content":{"encoding":"bytes","bytes":bytes}}]});
    assert!(binary::present(&context));
    binary::publish(&store, &state, &mut context).unwrap();
    let content = &context["touched_files"][0]["content"];
    assert_eq!(content["encoding"], "binary");
    assert_eq!(content["size"], 200000);
    assert!(content.get("bytes").is_none());
    assert!(
        !context["touched_files"][0]["patch"]
            .as_str()
            .unwrap()
            .contains("complete encoded payload")
    );
    let raw = content["source"]["$kah_source"].as_str().unwrap();
    source_store::verify_blob(&store.directory.join("sources").join(raw)).unwrap();
    let original_patch = content["git_diff_source"]["$kah_source"].as_str().unwrap();
    assert_eq!(
        fs::read(store.directory.join("sources").join(original_patch)).unwrap(),
        patch.as_bytes()
    );
    let output = read_back(&store.directory, &workspace, &context, raw);
    assert_eq!(output.stdout, bytes);
    drop(store);
    fs::remove_dir_all(Store::directory(&worker).unwrap().parent().unwrap()).unwrap();
}

fn local_input(context: &Value) -> kah_observer::commit_check::Input {
    let source = SourceRef::new(
        SourceProvider::Codex,
        "unit",
        "u1",
        SourceRole::User,
        "sha256:unit",
    );
    let history = History::new(GoalRevision::new(
        1,
        "Keep complete evidence.",
        vec![source],
    ))
    .unwrap();
    let rules = Resolved {
        source_digest: "unit".into(),
        observer_session: "same-observer".into(),
        history_digest: None,
        source_words: Default::default(),
        rules: vec![],
    };
    input::build(&history, &rules, context, false).unwrap()
}

fn read_back(
    private: &std::path::Path,
    workspace: &std::path::Path,
    context: &Value,
    raw: &str,
) -> std::process::Output {
    let view = workspace.join("inputs/current");
    fs::create_dir_all(&view).unwrap();
    source_store::initialize(private, workspace).unwrap();
    source_store::save(&view, "context.json", context).unwrap();
    fs::write(
        view.join("context-input.json"),
        json!({"sources":[],"full_request":view.join("context.json")}).to_string(),
    )
    .unwrap();
    source_store::share(private, &view).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kah"))
        .args(["observe", "read-blob"])
        .arg(workspace.join("sources").join(raw))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
