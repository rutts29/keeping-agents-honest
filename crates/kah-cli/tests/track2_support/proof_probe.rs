use super::{fixture, journey::Worker};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

pub fn run(worker: &Worker, sha: &str, phase: usize) -> Value {
    let root = worker.root.join(format!("probe-{}-{sha}", worker.provider));
    fs::create_dir_all(&root).unwrap();
    let mut digests = serde_json::Map::new();
    for name in [
        "PLAN.md",
        "phase1.cjs",
        "phase2.cjs",
        "integration.cjs",
        "normalizer.cjs",
        "serializer.cjs",
        "pipeline.cjs",
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&worker.project)
            .args(["show", &format!("{sha}:{name}")])
            .output()
            .unwrap();
        if !output.status.success() {
            assert!(
                phase == 1 && matches!(name, "serializer.cjs" | "pipeline.cjs"),
                "TRACK2_COMMITTED_FILE stage=probe: {name}"
            );
            continue;
        }
        digests.insert(
            name.into(),
            format!("{:x}", Sha256::digest(&output.stdout)).into(),
        );
        fs::write(root.join(name), output.stdout).unwrap();
    }
    let mut result = json!({"sha":sha,"committedDigests":digests,"serializer_absent":!root.join("serializer.cjs").exists(),"receipts":[]});
    for (name, key) in [
        ("phase1.cjs", "phase1"),
        ("phase2.cjs", "phase2"),
        ("integration.cjs", "integration"),
    ] {
        if phase == 1 && key != "phase1" {
            continue;
        }
        let output = Command::new("node")
            .arg(name)
            .current_dir(&root)
            .output()
            .unwrap();
        result[key] = output.status.success().into();
        result["receipts"].as_array_mut().unwrap().push(json!({"script":name,"exit":output.status.code(),
            "stdout":String::from_utf8(output.stdout).unwrap(),"stderr":String::from_utf8(output.stderr).unwrap()}));
    }
    fixture::save(&root.join("probe.json"), &result);
    result
}
