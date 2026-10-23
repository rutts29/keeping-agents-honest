//! Storage replay of an externally supplied corpus; no native agent or Jev request.
use kah_cli::observer_review::source_store;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
#[test]
#[ignore = "retained storage replay; KAH_RECORD_CAS_SOURCE and KAH_RECORD_CAS_EVIDENCE"]
fn retained_real_skeletons_reuse_records_without_changing_any_value() {
    let source = PathBuf::from(std::env::var_os("KAH_RECORD_CAS_SOURCE").unwrap());
    let ids: [String; 2] = serde_json::from_slice(
        &fs::read(source.parent().unwrap().join("fixture-records.json")).unwrap(),
    )
    .unwrap();
    let evidence = PathBuf::from(std::env::var_os("KAH_RECORD_CAS_EVIDENCE").unwrap());
    let base = std::env::var_os("KAH_RECORD_CAS_BENCH_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let root = base.join(format!("kah-record-retained-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let reader = root.join("reader");
    fs::create_dir(&reader).unwrap();
    std::os::unix::fs::symlink(&source, reader.join("sources")).unwrap();
    let probe = root.join("probe");
    fs::create_dir(&probe).unwrap();
    source_store::initialize(&probe, &root.join("workspace")).unwrap();
    for file in fs::read_dir(&source).unwrap() {
        let file = file.unwrap();
        let name = file.file_name();
        if !name.to_string_lossy().ends_with(".tmp") && file.file_type().unwrap().is_file() {
            fs::hard_link(file.path(), probe.join("sources").join(name)).unwrap();
        }
    }
    let before = inventory(&probe.join("sources"));
    let mut rows = vec![];
    let mut prior = before;
    for (index, id) in ids.iter().enumerate() {
        kah_cli::unattended_report::save(
            &reader,
            "input.json",
            &json!({"state":{"$kah_source":id,"encoding":"json"}}),
        )
        .unwrap();
        let original: Value = source_store::read(&reader.join("input.json")).unwrap();
        let start = Instant::now();
        let filename = format!("snapshot-{index}.json");
        source_store::save(&probe, &filename, &original).unwrap();
        let elapsed = start.elapsed().as_millis();
        assert_eq!(
            source_store::read::<Value>(&probe.join(&filename)).unwrap(),
            original
        );
        let next = inventory(&probe.join("sources"));
        rows.push(json!({"source":id,"logical_sha256":kah_observer::context_selection::digest(&original),"new_files":next.0-prior.0,"new_bytes":next.1-prior.1,"save_ms":elapsed,"roundtrip_equal":true}));
        prior = next;
    }
    kah_cli::unattended_report::save(evidence.parent().unwrap(),evidence.file_name().unwrap().to_str().unwrap(),&json!({"kind":"retained_storage_replay","seeded_existing_cas":true,"original_sources_unchanged":true,"rows":rows})).unwrap();
    fs::remove_dir_all(root).unwrap();
}
fn inventory(root: &Path) -> (u64, u64) {
    fs::read_dir(root)
        .unwrap()
        .map(|file| file.unwrap().metadata().unwrap().len())
        .fold((0, 0), |(n, bytes), size| (n + 1, bytes + size))
}
