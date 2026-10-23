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
/// (requirement id, requirement, code under test, the check's assertion)
const CASES: [(&str, &str, &str, &str); 4] = [
    (
        "normalize",
        "normalize trims every key",
        "const normalize = (s) => Object.fromEntries(Object.entries(s).map(([k, v]) => [k.trim(), v]));",
        "assert.deepEqual(normalize({' a ': 1}), {a: 1});",
    ),
    (
        "serialize",
        "serialize writes keys in sorted order",
        "const serialize = (s) => JSON.stringify(s, Object.keys(s).sort());",
        "assert.equal(serialize({b: 1, a: 2}), '{\"a\":2,\"b\":1}');",
    ),
    (
        "pipeline",
        "the pipeline normalizes, then serializes",
        "const pipeline = (s) => serialize(normalize(s));",
        "assert.equal(pipeline({' b ': 1, a: 2}), '{\"a\":2,\"b\":1}');",
    ),
    (
        "verification_assertions",
        "the check asserts both functions and the pipeline",
        "for (const t of [normalizeTest, serializeTest, pipelineTest]) t();",
        "console.log('VERIFY_PASS');",
    ),
];

#[test]
#[ignore = "live Jev check of reasoned coverage on authored evidence; opt-in, needs a Jev key"]
fn reasoned_coverage_on_authored_evidence() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let receipt = json!({"command":CHECK,"commit":HEAD,"exit_code":0,"success":true,"output":"VERIFY_PASS\n"});
    let mut outcomes = vec![];
    for (id, name, code, test) in CASES {
        let reasoning = format!(
            "{CHECK} runs `{test}` against `{code}` on this head, so it exercises: {name}."
        );
        let (state, questions) = case(id, name, code, test, &reasoning, &receipt);
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
        code,
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
