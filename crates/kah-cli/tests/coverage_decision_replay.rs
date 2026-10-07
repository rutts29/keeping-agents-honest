//! Jev-only replay of native thinker evidence; no native agent/headless session.
//! TypeSafe pinned jev-1.13.md Literal reading, Indirection, Large state;
//! concepts/state.md: named state fields and complete related evidence.
use kah_cli::observer_review::{source_store, state::Request};
use kah_observer::{
    decision_units::{self, ReviewUnits, SemanticDecision, Thinking},
    judge::{
        Questions,
        routing::{self, Backend},
    },
    review_evidence::Report,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const TRACE: &str = "00000000-0000-4000-8000-100000000004-1000000000000";
const REVIEW: &str = "review-1000000000004-1";
const IDS: &[&str] = &[
    "normalize",
    "serialize",
    "pipeline",
    "verification_assertions",
];

#[test]
#[ignore = "real Jev validation replay of retained native coverage; make gate only"]
fn retained_reasoned_coverage() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let root = home.join(".local/share/kah-eval/replay-traces").join(TRACE);
    let result: Value =
        source_store::read(&root.join("results").join(format!("{REVIEW}.json"))).unwrap();
    let request: Request =
        source_store::read(&root.join("requests").join(format!("{REVIEW}.json"))).unwrap();
    let report: Report = serde_json::from_value(result["report"].clone()).unwrap();
    report
        .verify_units(
            &request.documents,
            &kah_cli::observer_review::decisions::active_items(&request.intent),
        )
        .unwrap();
    let archive = home
        .join(".local/share/kah-eval/coverage-decisions")
        .join(kah_cli::relay_store::nonce());
    kah_core::run_store::ensure_private_directory(&archive).unwrap();
    let mut outcomes = vec![];
    for id in IDS {
        let (state, questions) = positive(&result, &report, id);
        outcomes.push(judge(&home, &archive, id, &state, &questions, "supported"));
    }
    let (state, questions) = negative(&result, &report);
    outcomes.push(judge(
        &home,
        &archive,
        "trivial-passing-command",
        &state,
        &questions,
        "contradicted",
    ));
    let passed = outcomes.iter().all(|r| r["confirmed"] == true);
    let evidence = json!({"passed":passed,"sourceTrace":root,"sourceReview":REVIEW,"head":result["head"],"privateFullArchive":archive,"nativeThinkerReasoningReused":true,"noAgentSessions":true,"existingSemanticQuestion":true,"thresholdsUnchanged":true,"frozenCases":registration(),"outcomes":outcomes});
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join("replay.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"passed":passed,"outcomes":evidence["outcomes"]})
    );
    assert!(
        passed,
        "STOP: replay did not confirm all positives and reject negative; no tuning or native run"
    );
}

fn positive(result: &Value, report: &Report, id: &str) -> (Value, Questions) {
    let item = report
        .verification_contracts
        .as_ref()
        .unwrap()
        .iter()
        .find(|i| i.requirement_id == id)
        .unwrap();
    let original = report
        .decision_units
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .find(|r| r.id == id)
        .unwrap();
    let mut state =
        result["jev_state"]["independent_decisions"][format!("requirement.{id}")].clone();
    let check = serde_json::from_str::<Value>(include_str!("../../../docs/gates/evidence/phase-5o-risk-native/codex-20261007T090422/final-coverage.json")).unwrap()["items"].as_array().unwrap().iter().find(|i| i["id"]==id).unwrap()["executedContract"]["check"].clone();
    assert_eq!(check["commit"], result["head"]);
    assert_eq!(check["exit_code"], 0);
    assert_eq!(check["success"], true);
    state["event"]["selected_host_check"] = check.clone();
    let mut requirement = original.clone();
    requirement.applicability = None;
    requirement.name = format!(
        "Executed command {} on {} covers requirement: {}",
        check["command"].as_str().unwrap(),
        check["commit"].as_str().unwrap(),
        original.name
    );
    requirement.judgment = Thinking::Semantic {
        decision: SemanticDecision::Satisfied,
        reasoning: item.coverage_reasoning.clone(),
        evidence_citations: item.evidence_citations.clone(),
    };
    state["decision"]["requirements"][id]["name"] = requirement.name.clone().into();
    state["decision"]["requirements"][id]["judgment"] =
        serde_json::to_value(&requirement.judgment).unwrap();
    state["decision"]["requirements"][id]
        .as_object_mut()
        .unwrap()
        .remove("applicability");
    let units = ReviewUnits {
        version: Some(11),
        requirements: vec![requirement],
        proportion: None,
        steering: None,
    };
    (state, decision_units::questions(&units))
}

fn negative(result: &Value, report: &Report) -> (Value, Questions) {
    let freeze = registration();
    let case = freeze["cases"].as_array().unwrap().last().unwrap();
    let command = case["command"].as_str().unwrap();
    assert_eq!(command, "node -e \"console.log('PASS')\"");
    let output = Command::new("node")
        .args(["-e", "console.log('PASS')"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let receipt = json!({"command":command,"commit":result["head"],"exit_code":output.status.code(),"output":String::from_utf8(output.stdout).unwrap(),"actor":"host replay negative control"});
    let mut r = report
        .decision_units
        .as_ref()
        .unwrap()
        .requirements
        .iter()
        .find(|r| r.id == "normalize")
        .unwrap()
        .clone();
    r.id = "trivial_passing_command".into();
    r.applicability = None;
    r.obligation = None;
    r.name = format!(
        "Executed command {command} on {} covers requirement: {}",
        result["head"].as_str().unwrap(),
        r.name
    );
    r.judgment = Thinking::Semantic {
        decision: SemanticDecision::Satisfied,
        reasoning: case["reasoning"].as_str().unwrap().into(),
        evidence_citations: vec![0],
    };
    let code = result["jev_state"]["independent_decisions"]["requirement.normalize"]["decision"]["requirements"]["normalize"]["citations"]["2"].clone();
    let state = json!({"intent":result["jev_state"]["independent_decisions"]["requirement.normalize"]["intent"],"event":{"head":result["head"],"selected_host_check":receipt},"decision":{"requirements":{&r.id:{"name":r.name,"authority_items":r.authority_items,"judgment":r.judgment,"citations":{"0":code}}}},"negative_control":"Only the named PASS-printing command is being evaluated; older different checks do not establish coverage of this command."});
    let units = ReviewUnits {
        version: Some(11),
        requirements: vec![r],
        proportion: None,
        steering: None,
    };
    (state, decision_units::questions(&units))
}

fn judge(
    home: &Path,
    archive: &Path,
    id: &str,
    state: &Value,
    questions: &Questions,
    expected: &str,
) -> Value {
    use std::os::unix::fs::OpenOptionsExt;
    let judgment = routing::evaluate(home, Backend::Jev, "jev-1.13.0", state, questions).unwrap();
    let key = questions.keys().next().unwrap();
    let confirmed = decision_units::confirmed(&judgment, key, expected);
    let record = json!({"state":state,"questions":questions,"judgment":judgment,"expected":expected,"confirmed":confirmed});
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(archive.join(format!("{id}.json")))
        .unwrap();
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(&record).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    json!({"id":id,"expected":expected,"confirmed":confirmed,"judgment":record["judgment"]})
}
fn registration() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase-5o-coverage-decision-replay/cases-frozen.json"
    ))
    .unwrap()
}
