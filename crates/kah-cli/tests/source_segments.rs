use kah_cli::observer_review::source_store;
use serde_json::{Value, json};
use std::fs;

#[test]
fn a_missing_segment_names_the_exact_file_and_preserves_the_manifest() {
    let root = std::env::temp_dir().join(format!("kah-missing-segment-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    source_store::text(
        &root,
        &root.join("transcript.txt"),
        &"complete record\n".repeat(9000),
    )
    .unwrap();
    let reference: Value =
        serde_json::from_slice(&fs::read(root.join("transcript.txt")).unwrap()).unwrap();
    let manifest_path = root
        .join("sources")
        .join(reference["$kah_source"].as_str().unwrap());
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let missing = root
        .join("sources")
        .join(format!("block-{}", manifest["blocks"][0].as_str().unwrap()));
    fs::remove_file(&missing).unwrap();
    let error = source_store::load_text(&root.join("transcript.txt"))
        .unwrap_err()
        .to_string();
    assert!(error.starts_with("REVIEW_SOURCE_IO stage=source_store"));
    assert!(error.contains(missing.to_str().unwrap()));
    assert!(manifest_path.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn growing_sources_share_segments_and_keep_exact_prefixes() {
    let temp = std::env::temp_dir().join(format!("kah-segments-{}", std::process::id()));
    fs::create_dir(&temp).unwrap();
    let root = temp.join("review");
    let workspace = temp.join("workspace");
    fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &workspace).unwrap();
    let prefix = "whole native record 🦀\n".repeat(20000);
    for i in 0..8 {
        let text = format!("{prefix}complete appended record {i}\n");
        source_store::save(
            &root,
            &format!("{i}.json"),
            &json!({"documents":[{"text":text}]}),
        )
        .unwrap();
    }
    let bytes: u64 = fs::read_dir(root.join("sources"))
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    assert!(
        bytes < prefix.len() as u64 * 3,
        "prefixes duplicated: {bytes}"
    );
    for i in 0..8 {
        let recovered: Value = source_store::read(&root.join(format!("{i}.json"))).unwrap();
        assert_eq!(
            recovered["documents"][0]["text"],
            format!("{prefix}complete appended record {i}\n")
        );
    }
    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn migration_verifies_old_snapshots_before_replacing_and_preserves_references() {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!("kah-segment-migration-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let sources = root.join("sources");
    fs::create_dir(&sources).unwrap();
    let original = "verbatim record 🦀\n".repeat(16000);
    let digest = format!("{:x}", Sha256::digest(original.as_bytes()));
    fs::write(sources.join(&digest), &original).unwrap();
    let reference = json!({"$kah_source":digest,"encoding":"utf8"});
    fs::write(
        root.join("transcript.txt"),
        serde_json::to_vec(&reference).unwrap(),
    )
    .unwrap();
    assert_eq!(source_store::segment_sources(&sources).unwrap(), 1);
    assert_eq!(
        source_store::load_text(&root.join("transcript.txt")).unwrap(),
        original
    );
    assert_eq!(source_store::segment_sources(&sources).unwrap(), 0);
    let manifest: Value =
        serde_json::from_slice(&fs::read(sources.join(&digest)).unwrap()).unwrap();
    assert_eq!(manifest["end"], original.len());
    let block = manifest["blocks"][0].as_str().unwrap();
    fs::write(sources.join(format!("block-{block}")), b"tampered").unwrap();
    assert!(
        source_store::load_text(&root.join("transcript.txt"))
            .unwrap_err()
            .to_string()
            .contains("REVIEW_SOURCE_DIGEST")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupt_legacy_snapshot_is_retained_and_migration_fails_closed() {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!("kah-segment-corrupt-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let digest = format!("{:x}", Sha256::digest(b"expected"));
    let path = root.join(digest);
    fs::write(&path, b"corrupt original retained").unwrap();
    assert!(source_store::segment_sources(&root).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt original retained");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn request_and_covered_trigger_payloads_are_losslessly_referenced() {
    let root = std::env::temp_dir().join(format!("kah-segment-payload-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    let trigger = json!({"actions":[{"output":"complete tool result\n".repeat(5000)}],"reconsideration":{"why":"preserve every word"}});
    let request = json!({"id":"review-whole","trigger":trigger,"documents":[{"id":"transcript","text":"full transcript","native_records":[{"start_byte":0,"end_byte":15,"context":{"role":"user"}}]}]});
    let covered = json!({"trigger":trigger,"purpose":"drift"});
    source_store::save(&root, "request.json", &request).unwrap();
    source_store::save(&root, "covered.json", &covered).unwrap();
    for (name, original) in [("request.json", request), ("covered.json", covered)] {
        assert!(fs::metadata(root.join(name)).unwrap().len() < 1024);
        assert_eq!(
            source_store::read::<Value>(&root.join(name)).unwrap(),
            original
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_retains_original_if_a_shared_block_is_corrupt() {
    use sha2::{Digest, Sha256};
    let root =
        std::env::temp_dir().join(format!("kah-segment-block-corrupt-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let original = vec![b'x'; 100000];
    let digest = format!("{:x}", Sha256::digest(&original));
    let block_digest = format!("{:x}", Sha256::digest(&original[..65536]));
    let path = root.join(&digest);
    fs::write(&path, &original).unwrap();
    fs::write(
        root.join(format!("block-{block_digest}")),
        b"corrupt block retained",
    )
    .unwrap();
    assert!(source_store::segment_sources(&root).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trace_archival_verifies_source_manifests_and_binary_blocks() {
    let root = std::env::temp_dir().join(format!("kah-segment-archive-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    source_store::initialize(&root, &root.join("workspace")).unwrap();
    source_store::save(
        &root,
        "request.json",
        &json!({"documents":[{"text":"whole record 🦀\n".repeat(10000)}]}),
    )
    .unwrap();
    let mut block = None;
    for entry in fs::read_dir(root.join("sources")).unwrap() {
        let entry = entry.unwrap();
        source_store::verify_blob(&entry.path()).unwrap();
        if entry.file_name().to_string_lossy().starts_with("block-") {
            block = Some(entry.path());
        }
    }
    let block = block.unwrap();
    fs::write(&block, b"corrupt block retained").unwrap();
    assert!(
        source_store::verify_blob(&block)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_SOURCE_DIGEST")
    );
    fs::remove_dir_all(root).unwrap();
}
