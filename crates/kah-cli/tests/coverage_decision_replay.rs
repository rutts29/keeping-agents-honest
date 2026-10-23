//! Opt-in live Jev check of reasoned coverage on authored evidence: a real
//! check that exercises each requirement supports it; a command that only
//! prints PASS does not. No native agent or session.
//! TypeSafe pinned jev-1.13.md Literal reading, Indirection, Large state;
//! concepts/state.md: named state fields and complete related evidence.
use kah_observer::{
    decision_units::{self, Requirement, ReviewUnits},
    judge::{
        Questions,
        routing::{self, Backend},
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, process::Command};

const HEAD: &str = "8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c";
const CHECK: &str = "node verify.cjs";
/// The complete check the receipts come from. It runs and passes.
const VERIFY: &str = r#"const assert = require('node:assert/strict');
const normalize = (s) => Object.fromEntries(Object.entries(s).map(([k, v]) => [k.trim(), v]));
const serialize = (s) => JSON.stringify(s, Object.keys(s).sort());
const pipeline = (s) => serialize(normalize(s));
const normalizeTest = () => assert.deepEqual(normalize({ ' a ': 1, b: 2 }), { a: 1, b: 2 });
const serializeTest = () => assert.equal(serialize({ b: 1, a: 2 }), '{"a":2,"b":1}');
const pipelineTest = () => assert.equal(pipeline({ ' b ': 1, a: 2 }), '{"a":2,"b":1}');
for (const t of [normalizeTest, serializeTest, pipelineTest]) t();
console.log('VERIFY_PASS');
"#;
/// (requirement id, requirement, cited code lines, cited check lines), 1-based in VERIFY.
/// First and last line, inclusive.
type Lines = (usize, usize);
const CASES: [(&str, &str, Lines, Lines); 4] = [
    ("normalize", "normalize trims every key", (2, 2), (5, 5)),
    (
        "serialize",
        "serialize writes keys in sorted order",
        (3, 3),
        (6, 6),
    ),
    (
        "pipeline",
        "the pipeline normalizes, then serializes",
        (2, 4),
        (7, 7),
    ),
    (
        "verification_assertions",
        "the check asserts both functions and the pipeline",
        (5, 7),
        (8, 9),
    ),
];

fn lines((first, last): Lines) -> String {
    VERIFY.lines().collect::<Vec<_>>()[first - 1..last].join("\n")
}
/// Runs VERIFY with node in a temporary directory; the receipt is its real result.
fn run_verify() -> Value {
    let dir = std::env::temp_dir().join(format!("coverage-verify-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("verify.cjs"), VERIFY).unwrap();
    let output = Command::new("node")
        .arg("verify.cjs")
        .current_dir(&dir)
        .output()
        .unwrap();
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(
        output.status.success() && stdout == "VERIFY_PASS\n",
        "{output:?}"
    );
    json!({"command":CHECK,"commit":HEAD,"exit_code":output.status.code(),"success":true,"output":stdout})
}

#[test]
fn the_authored_check_runs_and_passes() {
    assert_eq!(run_verify()["exit_code"], 0);
}

#[test]
#[ignore = "live Jev check of reasoned coverage on authored evidence; opt-in, needs a Jev key"]
fn reasoned_coverage_on_authored_evidence() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let receipt = run_verify();
    let mut outcomes = vec![];
    for (id, name, code, test) in CASES {
        let (code, test) = (lines(code), lines(test));
        let reasoning = format!(
            "{CHECK} runs `{test}` against `{code}` on this head and exits 0, so it exercises: {name}."
        );
        let (state, questions) = case(id, name, &code, &test, &reasoning, &receipt);
        outcomes.push(judge(&home, id, &state, &questions, true));
    }
    // Negative control: a command that only prints PASS.
    let output = Command::new("node")
        .args(["-e", "console.log('PASS')"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let pass_only = json!({"command":"node -e \"console.log('PASS')\"","commit":HEAD,"exit_code":0,
        "success":true,"output":String::from_utf8(output.stdout).unwrap()});
    let (_, name, code, _) = CASES[0];
    let (state, questions) = case(
        "trivial_passing_command",
        name,
        &lines(code),
        "console.log('PASS')",
        "The command exits 0 and prints PASS, so the requirement is covered.",
        &pass_only,
    );
    outcomes.push(judge(
        &home,
        "trivial-passing-command",
        &state,
        &questions,
        false,
    ));
    let passed = outcomes.iter().all(|r| r["confirmed"] == true);
    if let Some(evidence) = std::env::var_os("KAH_GATE_EVIDENCE") {
        fs::write(
            PathBuf::from(evidence).join("replay.json"),
            serde_json::to_vec_pretty(&json!({"passed":passed,"head":HEAD,"outcomes":outcomes}))
                .unwrap(),
        )
        .unwrap();
    }
    assert!(
        passed,
        "positives not all supported or the PASS-only control was supported: {outcomes:?}"
    );
}

fn citation(source: &str, quote: &str) -> Value {
    json!({"source":source,"quote":quote,"digest":format!("sha256:{:x}", Sha256::digest(quote))})
}
fn case(
    id: &str,
    name: &str,
    code: &str,
    test: &str,
    reasoning: &str,
    receipt: &Value,
) -> (Value, Questions) {
    let named = format!(
        "Executed command {} on {HEAD} covers requirement: {name}",
        receipt["command"].as_str().unwrap()
    );
    let citations = json!([citation("code", code), citation("check", test)]);
    let requirement: Requirement = serde_json::from_value(json!({"id":id,"name":named,
        "dimension":"intent","authority_items":["goal"],"citations":citations,"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":reasoning,"evidence_citations":[0, 1]}}))
    .unwrap();
    let state = json!({"intent":{"goal":"Build a settings normalizer, serializer and pipeline with a check.",
            "items":[{"id":"goal","text":name}]},
        "event":{"head":HEAD,"selected_host_check":receipt},
        "decision":{"requirements":{id:{"name":named,"authority_items":["goal"],
            "judgment":requirement.judgment,"citations":{"0":citations[0],"1":citations[1]}}}}});
    let units = ReviewUnits {
        version: Some(11),
        requirements: vec![requirement],
        proportion: None,
        steering: None,
    };
    (state, decision_units::questions(&units))
}

fn judge(
    home: &std::path::Path,
    id: &str,
    state: &Value,
    questions: &Questions,
    supported: bool,
) -> Value {
    let judgment = routing::evaluate(home, Backend::Jev, "jev-1.13.0", state, questions).unwrap();
    let key = questions.keys().next().unwrap();
    let confirmed = decision_units::confirmed(&judgment, key, "supported") == supported;
    json!({"id":id,"expectedSupported":supported,"confirmed":confirmed,"judgment":judgment})
}
