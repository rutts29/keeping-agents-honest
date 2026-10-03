use kah_cli::observer_review::source_store;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::MetadataExt};

#[test]
fn preparation_and_archive_recover_whole_sources_from_one_copy() {
    let temp = std::env::temp_dir().join(format!("kah-source-test-{}", std::process::id()));
    fs::create_dir(&temp).unwrap();
    let private = temp.join("review");
    let workspace = temp.join("workspace");
    fs::create_dir_all(&private).unwrap();
    source_store::initialize(&private, &workspace).unwrap();
    let original = json!({"user_text":"whole decisive words 🦀", "documents":[{"text":"the whole repeated transcript"}]});
    let job = json!({"state":original,"items":[{"digest":"unchanged"}]});
    source_store::save(&private, "preparation.json", &job).unwrap();
    let input = workspace.join("inputs/request");
    fs::create_dir_all(&input).unwrap();
    source_store::save(&input, "preparation.json", &job).unwrap();
    let archive =
        json!({"original":original,"prepared_state":original,"selection":{"selected":["whole"]}});
    source_store::save(&private, "prepared.json", &archive).unwrap();
    let records: Value =
        serde_json::from_slice(&fs::read(private.join("prepared.json")).unwrap()).unwrap();
    assert_eq!(records["original"], records["prepared_state"]);
    assert!(records["original"]["$kah_source"].is_string());
    assert_eq!(
        source_store::read::<Value>(&private.join("preparation.json")).unwrap(),
        job
    );
    assert_eq!(
        source_store::read::<Value>(&private.join("prepared.json")).unwrap(),
        archive
    );
    // The public alias shares the inode; no extra physical source copy.
    alias_sources(&private, &workspace);
    assert_eq!(
        source_store::read::<Value>(&input.join("preparation.json")).unwrap(),
        job
    );
    source_store::text(
        &private,
        &input.join("transcript.txt"),
        "the whole repeated transcript",
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(input.join("transcript.txt")).unwrap(),
        "the whole repeated transcript"
    );
    // Existing corrupt content fails closed before any record replacement.
    let digest = records["original"]["$kah_source"].as_str().unwrap();
    fs::write(private.join("sources").join(digest), b"corrupt").unwrap();
    assert!(
        source_store::read::<Value>(&private.join("prepared.json"))
            .unwrap_err()
            .to_string()
            .contains("REVIEW_SOURCE_DIGEST")
    );
    fs::remove_dir_all(temp).unwrap();
}
fn alias_sources(private: &std::path::Path, workspace: &std::path::Path) {
    for entry in fs::read_dir(private.join("sources")).unwrap() {
        let entry = entry.unwrap();
        let alias = workspace.join("sources").join(entry.file_name());
        fs::hard_link(entry.path(), &alias).unwrap();
        assert_eq!(
            fs::metadata(entry.path()).unwrap().ino(),
            fs::metadata(alias).unwrap().ino()
        );
    }
}

#[test]
fn queued_preparation_and_latest_judgment_reference_nested_sources_too() {
    let root = std::env::temp_dir().join(format!("kah-nested-source-test-{}", std::process::id()));
    let workspace = root.join("workspace");
    fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &workspace).unwrap();
    let full =
        json!({"goal":"Keep every verbatim word.","items":["Never cut or silently drop an item."]});
    let queued = json!({"trigger":{"preparation":{"state":full}}});
    let latest = json!({"latest":{"jev_state":full}});
    source_store::save(&root, "queued.json", &queued).unwrap();
    source_store::save(&root, "latest.json", &latest).unwrap();
    let q: Value = serde_json::from_slice(&fs::read(root.join("queued.json")).unwrap()).unwrap();
    let l: Value = serde_json::from_slice(&fs::read(root.join("latest.json")).unwrap()).unwrap();
    assert_eq!(
        q["trigger"]["preparation"]["state"],
        l["latest"]["jev_state"]
    );
    assert_eq!(
        source_store::read::<Value>(&root.join("queued.json")).unwrap(),
        queued
    );
    assert_eq!(
        source_store::read::<Value>(&root.join("latest.json")).unwrap(),
        latest
    );
    fs::remove_dir_all(root).unwrap();
}
