//! The current B2 positive explicitly requests a gate; completion alone is insufficient.
use super::*;

pub fn request_gate(home: &Path, id: &str, record: &kah_run::store::Record) {
    if std::env::var_os("KAH_PHASE3_B2").is_none()
        || !record.worktree.join("verification.cjs").is_file()
    {
        return;
    }
    let Some(worker) = record.native.as_ref().and_then(|n| n.session.as_deref()) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
    let marker = root.join("phase3-gate-requested.json");
    if marker.exists() {
        return;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&record.worktree)
        .args(["show", "HEAD:verification.cjs"])
        .output()
        .unwrap();
    if !output.status.success() {
        return;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&record.worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let head = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    fs::write(
        &marker,
        json!({"run":id,"worker":worker,"head":head,"status":"reserved"}).to_string(),
    )
    .unwrap();
    let output = Command::new(home.join(".local/bin/kah"))
        .args(["gate", worker])
        .output()
        .unwrap();
    fs::write(
        root.join("phase3-gate-cli.json"),
        json!({"worker":worker,"head":head,
        "exit":output.status.code(),"stdout":String::from_utf8(output.stdout).unwrap(),
        "stderr":String::from_utf8(output.stderr).unwrap()})
        .to_string(),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "PHASE3_GATE_REQUEST stage=explicit_gate"
    );
}

pub fn score(result: &mut Value, directory: &Path) {
    if std::env::var_os("KAH_PHASE3_B2").is_none() {
        return;
    }
    let typed = fs::read_dir(directory.join("requests"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|s| s == "json"))
        .filter_map(|e| {
            let request: Value = kah_cli::observer_review::source_store::read(&e.path()).unwrap();
            (request["worker"] == result["worker"]
                && request["head"] == result["checkpoint"]["head"]
                && request["trigger"]["option1_cumulative_input"].is_object())
            .then(|| {
                json!({"request":request["id"],"head":request["head"],
                    "cumulative_input":request["trigger"]["option1_cumulative_input"]})
            })
        })
        .collect::<Vec<_>>();
    result["passed"] = json!(
        result["passed"] == true
            && result["questionSeen"] == false
            && result["checkpoint"]["passed"] == true
            && !typed.is_empty()
    );
    result["phase3"] = json!({"case":"VF-GOOD","typed_gate_requests":typed,
        "other_B2_cases":"risk accept/reject, violation and no-recorded-check still required",
        "explicit_gate_proven":!typed.is_empty()});
}
