//! Fixed-task paired state-construction measurements; rejected completion is data.
use super::*;
use kah_observer::judge;

pub(super) fn finish(
    home: &Path,
    record: &Record,
    report: &Value,
    before: &[(PathBuf, Vec<u8>)],
) -> Value {
    let native = record.native.as_ref().unwrap();
    let session = native.session.as_ref().unwrap();
    assert_eq!(report["tests"]["success"], true);
    assert!(matches!(
        record.published_status(),
        RunStatus::Completed | RunStatus::Stopped
    ));
    if record.published_status() == RunStatus::Stopped {
        assert!(
            report["error"]
                .as_str()
                .unwrap()
                .starts_with("RUN_NO_COMPLETION")
        );
    }
    let directory =
        kah_observer::session::Store::directory(&RunStore::default_state_root().unwrap(), session);
    proof::native_tools(home, &directory, record);
    let captures = captures(&directory, report["tests"]["commit"].as_str().unwrap());
    let first = &captures[captures.len() - 2];
    let second = &captures[captures.len() - 1];
    assert_eq!(first["channel"], "activity");
    assert_eq!(second["channel"], "artifact");
    assert_eq!(
        first["request"]["state"]["intent"],
        second["request"]["state"]["intent"]
    );
    assert_eq!(
        first["request"]["questions"],
        second["request"]["questions"]
    );
    assert_ne!(first["request_digest"], second["request_digest"]);
    assert!(
        second["request"]["state"]["event"]["actions"]
            .get("observed_activity")
            .is_none()
    );
    let outcomes: Vec<(String, String)> =
        serde_json::from_value(first["request"]["state"]["intent"]["outcomes"].clone()).unwrap();
    let replay =
        kah_cli::completion_replay::run(home, Path::new(first["archivePath"].as_str().unwrap()))
            .unwrap();
    let baseline: Value = serde_json::from_slice(&fs::read(&replay).unwrap()).unwrap();
    assert_eq!(baseline["request"], first["request"]);
    let old_repeat: judge::Judgment = serde_json::from_value(baseline["judgment"].clone()).unwrap();
    let first: judge::Judgment = serde_json::from_value(first["judgment"].clone()).unwrap();
    let second: judge::Judgment = serde_json::from_value(second["judgment"].clone()).unwrap();
    let old_complete = judge::decision::complete(&first, &outcomes)
        && judge::decision::complete(&old_repeat, &outcomes);
    let new_complete = judge::decision::complete(&first, &outcomes)
        && judge::decision::complete(&second, &outcomes);
    assert_eq!(first.model, second.model);
    assert_eq!(first.model, old_repeat.model);
    for (path, bytes) in before {
        assert_eq!(*bytes, fs::read(path).unwrap());
    }
    json!({"passed":true,"agent":native.provider,"version":version(home,&native.provider),"fixtureModel":record.model,"fixtureEffort":record.effort,"trial":std::env::var("KAH_GATE_COMPLETION_TRIAL").unwrap(),
        "independentTestsPassed":true,"controllerStatus":record.published_status(),"oldConstructionComplete":old_complete,"newConstructionComplete":new_complete,"oldFalseStop":!old_complete,"newFalseStop":!new_complete,
        "sameFirstJudgment":true,"questionsAndThresholdsUnchanged":true,"privateRequestsCaptured":captures.len(),"baselineReplay":replay,"jevModel":first.model,
        "judgments":{"sharedFirst":first,"oldSecond":old_repeat,"newSecond":second},"consentRequired":native.provider=="claude","globalConfigUnchanged":true})
}
fn captures(directory: &Path, commit: &str) -> Vec<Value> {
    let values = fs::read_to_string(directory.join("observations.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            row["source"] == "completion" && row["kind"] == "judgment" && row["commit"] == commit
        })
        .map(|row| {
            let mut capture: Value =
                serde_json::from_slice(&fs::read(row["replay"].as_str().unwrap()).unwrap())
                    .unwrap();
            capture["archivePath"] = row["replay"].clone();
            capture
        })
        .collect::<Vec<_>>();
    assert!(values.len() >= 2);
    values
}
