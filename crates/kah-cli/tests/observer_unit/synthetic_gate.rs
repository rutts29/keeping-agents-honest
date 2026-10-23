//! A small synthetic v12 gate: a user goal, one authority item, a settings
//! formatter and named requirements whose citations are bound to these
//! documents. Ids, digests and citations derive from this text alone.
// Each test module includes this builder and uses only its own fixtures.
#![allow(dead_code)]
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command};

pub const HEAD: &str = "4f1c2a9e8b7d6c5f4e3d2c1b0a9f8e7d6c5b4a39";
pub const REQUEST: &str = "review-1700000002000-7";
pub const RECEIPT: &str =
    "receipt-0d6c3a1f9e8b7a6d5c4b3a29180f7e6d5c4b3a2918f7e6d5c4b3a2918f7e6d5c.json";
pub const TAG: &str = "gate-ready";
const BLOB: &str = "2b7e151628aed2a6abf7158809cf4f3c762e7160";
const GOAL: &str = "Build normalize.cjs that rounds each number to two decimals and serialize.cjs that writes a JSON array, then pipeline.cjs that runs normalize before serialize. Add verification.cjs that checks all three and record the plan in PLAN.md.";
const AUTHORITY: &str = "Do not push or deploy.";
const CODE: &str = "normalize.cjs\nfunction normalize(values) {\n  return values.map((n) => Math.round(n * 100) / 100);\n}\nmodule.exports = normalize;\nserialize.cjs\nmodule.exports = (values) => JSON.stringify(values);\npipeline.cjs\nconst normalize = require('./normalize.cjs');\nconst serialize = require('./serialize.cjs');\nmodule.exports = (values) => serialize(normalize(values));\nverification.cjs\nconst assert = require('node:assert/strict');\nassert.equal(require('./pipeline.cjs')([1.234]), '[1.23]');\nconsole.log('verification ok');\n";
const PLAN: &str =
    "# Plan\nPhase 1: normalize, serialize and the pipeline, checked by verification.cjs.\n";
/// One worker shell body that edits, checks, commits and tags at once.
pub const COMMAND: &str = "cat > normalize.cjs <<'E'\nmodule.exports = (values) => values.map((n) => Math.round(n * 100) / 100);\nE\nnode verification.cjs && git commit -qam 'Add the pipeline' && git tag gate-ready";
const SAID: &str = "assistant: ";
const TAIL: &str = "\ntool: verification ok\nassistant: The work is committed locally; nothing was pushed or deployed.\n";
const EVENT: &str = "{\"kind\":\"user_gate\",\"source\":\"host_cli\"}";

pub fn document(id: &str, kind: &str, uri: &str, text: &str) -> Value {
    serde_json::to_value(Document::new(id, kind, uri, text)).unwrap()
}
/// A citation of `quote` in `document`; the quote must be in its text.
pub fn cite(document: &Value, quote: &str) -> Value {
    let text = document["text"].as_str().unwrap();
    assert!(text.contains(quote), "{quote}");
    json!({"source":document["id"],"digest":document["digest"],"quote":quote,"occurrence":0})
}
/// The worker transcript: native records cover the shell body and its output.
fn transcript() -> Value {
    let text = format!("{SAID}{COMMAND}{TAIL}");
    let mut document = document("transcript", "worker_transcript", "native:worker", &text);
    let output = text.find("verification ok").unwrap();
    let record = |start: usize, end: usize, kind: &str, content: &str| {
        json!({"start_byte":start,"end_byte":end,"context":{"provider":"claude","record_type":kind,
            "message_role":kind,"content_types":[content],"native_transport_only":true,"is_meta":null,
            "is_sidechain":false,"output_ranges":[],"path":"native/1","record_id":format!("record-{start}"),
            "native_timestamp":null,"turn_id":null,"status":null,"exit_code":null,"executions":[]}})
    };
    let mut command = record(
        SAID.len(),
        SAID.len() + COMMAND.len(),
        "assistant",
        "tool_use",
    );
    command["context"]["executions"] = json!([{"id":"tool-1","command":COMMAND,"cwd":"/work",
        "result":false,"result_digest":null}]);
    let mut result = record(
        output,
        output + "verification ok".len(),
        "user",
        "tool_result",
    );
    result["context"]["output_ranges"] = json!([[output, output + "verification ok\n".len()]]);
    document["native_records"] = json!([command, result]).to_string().into();
    document
}
/// Indexes: 0 ledger, 1 transcript, 2 code, 3 plan, 4 artifact, 5 event, 6 git.
pub fn documents(head: &str) -> Vec<Value> {
    let ledger = format!("goal: {GOAL}\nuser-1: {AUTHORITY}\n");
    let artifact =
        json!({"path":"PLAN.md","baseline_blob":BLOB,"delivered_blob":BLOB,"delivered_sha":head});
    vec![
        document("ledger", "ledger", "kah:ledger", &ledger),
        transcript(),
        document(
            "committed-snapshot",
            "committed_code",
            "git:HEAD-complete-tree",
            CODE,
        ),
        document("plan-0", "plan", "PLAN.md", PLAN),
        document(
            "artifact-0",
            "controller_artifact_digests",
            &format!("git:{head}:PLAN.md"),
            &artifact.to_string(),
        ),
        document(
            "checkpoint-event",
            "controller_event",
            "kah:review-trigger",
            EVENT,
        ),
        document(
            "git",
            "controller_facts",
            "git:context",
            &json!({"head":head,"tags":format!("{head} refs/tags/{TAG}")}).to_string(),
        ),
    ]
}
pub fn intent() -> Value {
    json!({"revision":2,"goal":GOAL,"items":[{"id":"user-1","text":AUTHORITY,"source":[]}],
        "goal_source":[],"open_deferrals":[],"open_outcomes":[],"completed_outcomes":[],"failures":[]})
}
/// The controller proof; with checks it holds one passing exact-SHA test.
pub fn proof(head: &str, recorded_checks_absent: bool) -> Value {
    let tests = if recorded_checks_absent {
        json!([])
    } else {
        json!([{"command":"node verification.cjs","commit":head,"exit_code":0,"success":true,"output":"verification ok\n"}])
    };
    json!({"head":head,"fresh_checkout":true,"tests":tests,"recorded_checks_absent":recorded_checks_absent,
        "artifacts":[{"path":"PLAN.md","baseline_blob":BLOB,"delivered_blob":BLOB,"delivered_sha":head}]})
}
pub fn request(worker: &str, head: &str) -> Value {
    json!({"id":REQUEST,"worker":worker,"purpose":"checkpoint","turn":"turn-gate","head":head,
        "intent":intent(),"documents":documents(head),"created_ms":1_700_000_002_000_u64,
        "trigger":{"decision_protocol_version":12,"review_class":"gate",
            "events":[{"kind":"user_gate","source":"host_cli","at_ms":1_700_000_001_900_u64}],
            "interactive_gate_verification":{"proof":proof(head, true)}}})
}
fn applicability(evidence: &[usize], unconditional: bool) -> Value {
    json!({"due":true,"unconditional":unconditional,"phase_completion":false,"controller_due":null,
        "evidence_citations":evidence,"reasoning":"The current phase names this requirement and the gate reviews it now."})
}
/// One satisfied semantic requirement: the goal, code quotes, then the event.
pub fn semantic(head: &str, id: &str, name: &str, dimension: &str, code: &[&str]) -> Value {
    let docs = documents(head);
    let mut citations = vec![cite(&docs[0], GOAL)];
    citations.extend(code.iter().map(|q| cite(&docs[2], q)));
    let evidence = (0..citations.len()).collect::<Vec<_>>();
    citations.push(cite(&docs[5], EVENT));
    json!({"id":id,"name":name,"dimension":dimension,"authority_items":["goal"],"checks":[],
        "citations":citations,"components":[{"name":"pipeline.cjs","role":"implementation"}],
        "applicability":applicability(&[evidence.len()], false),
        "judgment":{"kind":"semantic","decision":"satisfied","evidence_citations":evidence,
            "reasoning":format!("The committed code shows that {name}.")}})
}
fn obligation(mut requirement: Value, kind: &str) -> Value {
    requirement["obligation"] = json!({"kind":kind,"evidence_citations":[0],
        "reasoning":"The linked obligation owns routing for this requirement."});
    requirement
}
/// The unconditional authority item: observed history, never a check.
fn no_push(head: &str, id: &str) -> Value {
    let docs = documents(head);
    let mut item = semantic(head, id, "work is not pushed or deployed", "intent", &[]);
    item["authority_items"] = json!(["user-1"]);
    item["citations"] = json!([
        cite(&docs[0], AUTHORITY),
        cite(&docs[1], "nothing was pushed or deployed")
    ]);
    item["applicability"] = applicability(&[0], true);
    item["judgment"]["evidence_citations"] = json!([0, 1]);
    item["obligation"] = json!({"kind":"non_executable","evidence_citations":[1],"reasoning":"Observed history, not an executable artifact."});
    item
}
fn contract(id: &str, claim: &str, method: Value) -> Value {
    json!({"requirement_id":id,"claim":claim,"coverage_reasoning":format!("The planned method exercises that {claim}."),
        "evidence_citations":[0, 1],"method":method})
}
fn check(command: &str) -> Value {
    json!({"kind":"targeted_check","command":command})
}
fn report(requirements: Vec<Value>, contracts: Vec<Value>) -> Value {
    json!({"request_id":REQUEST,"findings":[],"verification_risks":[],"verification_contracts":contracts,
        "reminder":{"instruction":"Keep the work local.","quotes":[{"id":"user-1","quote":AUTHORITY}]},
        "decision_units":{"version":12,"proportion":null,"requirements":requirements}})
}
/// The first-round fixture: an initial plan with no recorded checks; three
/// named requirements still need thought.
fn first_round_requirements(h: &str) -> Vec<Value> {
    let docs = documents(h);
    let mut preserve = semantic(
        h,
        "req_preserve_plan",
        "PLAN.md is preserved",
        "intent",
        &[],
    );
    preserve["citations"] = json!([
        cite(&docs[0], GOAL),
        cite(&docs[4], BLOB),
        cite(&docs[5], EVENT)
    ]);
    preserve["applicability"] = applicability(&[2], false);
    preserve["judgment"] = json!({"kind":"artifact_preservation","evidence_citations":[0, 1],
        "fact":{"receipt":{"path":"PLAN.md","baseline_blob":BLOB,"delivered_blob":BLOB,"delivered_sha":h},"receipt_citation":1}});
    let drift = semantic(
        h,
        "req_drift",
        "the work did not drift from the plan",
        "drift",
        &[],
    );
    vec![
        semantic(
            h,
            "req_normalize",
            "normalize rounds to two decimals",
            "intent",
            &["Math.round(n * 100) / 100"],
        ),
        semantic(
            h,
            "req_serialize",
            "serialize writes a JSON array",
            "intent",
            &["JSON.stringify(values)"],
        ),
        semantic(
            h,
            "req_verification",
            "verification.cjs checks the pipeline",
            "intent",
            &["console.log('verification ok');"],
        ),
        semantic(h, "req_plan", "PLAN.md records the plan", "intent", &[]),
        preserve,
        no_push(h, "req_no_push_deploy"),
        semantic(
            h,
            "req_integration",
            "the pipeline runs normalize before serialize",
            "integration",
            &["serialize(normalize(values))"],
        ),
        obligation(drift, "non_executable"),
    ]
}
pub fn first_round(worker: &str) -> Value {
    let h = HEAD;
    let requirements = first_round_requirements(h);
    let contracts = vec![
        contract(
            "req_normalize",
            "normalize rounds to two decimals",
            check("node -e \"require('./normalize.cjs')\""),
        ),
        contract(
            "req_serialize",
            "serialize writes a JSON array",
            check("node -e \"require('./serialize.cjs')\""),
        ),
        contract(
            "req_verification",
            "verification.cjs passes",
            check("node verification.cjs"),
        ),
        contract(
            "req_plan",
            "PLAN.md names phase 1",
            check("grep -q 'Phase 1' PLAN.md"),
        ),
        contract(
            "req_integration",
            "the pipeline output is normalized",
            check("node verification.cjs"),
        ),
    ];
    json!({"request":request(worker, h),"report":report(requirements, contracts),"proof":proof(h, true),
        "assessment":{"passed":null,"state":"checkpoint_missing_recorded_checks","violations":[],
            "needs_user":[],"needs_thinker":["req_verification","req_plan","req_drift"]},
        "recorded_controller_check":{"command":"node verification.cjs","commit":h,"exit_code":0,
            "success":true,"output":"verification ok\n","output_truncated":false,"elapsed_ms":40},
        "calibration":{"input_tokens":2900,"input_utf8_bytes":6400,"model":"jev-1.13.0","planning_only":true}})
}
/// The verifier fixture: one semantic item and one recorded output-line fact
/// whose check cites the mixed worker shell body.
pub fn verifier(worker: &str) -> Value {
    let h = HEAD;
    let docs = documents(h);
    let normalize = semantic(
        h,
        "normalize-behavior",
        "normalize rounds to two decimals",
        "intent",
        &["Math.round(n * 100) / 100"],
    );
    let git = docs[6]["text"].as_str().unwrap().to_owned();
    let line = json!({"name":"node verification.cjs","command_citation":1,"result_citation":2,"sha":h,"sha_citation":3});
    let pass = json!({"id":"verification-pass-line","name":"verification.cjs prints its pass line","dimension":"intent",
        "authority_items":["goal"],"components":[{"name":"verification.cjs","role":"test"}],"checks":[line],
        "citations":[cite(&docs[0], GOAL), cite(&docs[1], COMMAND), cite(&docs[1], "verification ok"),
            cite(&docs[6], &git), cite(&docs[5], EVENT)],
        "applicability":applicability(&[4], false),
        "judgment":{"kind":"atomic_fact","evidence_citations":[1, 2],"fact":{"check":0,"property":"output_line","expected":"verification ok"}}});
    let contracts = vec![
        contract(
            "normalize-behavior",
            "normalize rounds to two decimals",
            check(
                "node -e \"require('node:assert/strict').deepEqual(require('./normalize.cjs')([1.234]),[1.23])\"",
            ),
        ),
        contract(
            "verification-pass-line",
            "verification.cjs prints its pass line",
            check("node verification.cjs | grep -qx 'verification ok'"),
        ),
    ];
    let requirements = vec![normalize, pass, no_push(h, "no-push-deploy")];
    json!({"request":request(worker, h),"report":report(requirements, contracts),"proof":proof(h, false),"state":state(worker, h)})
}
/// The observer state while a fixture checkpoint is pending.
pub fn state(worker: &str, head: &str) -> Value {
    json!({"worker":{"provider":"claude","session":worker,"project":"/work","pane":null,"socket":null},
        "observer":{"session":"observer-1","pane":null,"name":"observer","workspace":"/work/observer",
            "model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":{"id":REQUEST,"purpose":"checkpoint","turn":"turn-gate","head":head,"created_ms":1_700_000_002_000_u64,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":{"head":head,"intent_revision":2,"passed":null,"request":REQUEST,"state":"verify_first_pending"},
        "deferrals":[],"wakes":1,"native_tokens":0,"stopped":false})
}
/// A request after its single verification round, admitted from a typed gate job.
pub fn followup_request(worker: &str, head: &str) -> Value {
    let mut request = request(worker, head);
    let trigger = &mut request["trigger"];
    trigger["option1_cumulative_input"] =
        json!({"head":head,"intent_revision":2,"request":"review-1700000001500-3"});
    trigger["queued_at_ms"] = json!(1_700_000_001_950_u64);
    trigger["queued_receipt"] = json!(RECEIPT);
    trigger["interactive_gate_verification"] = json!({"proof":proof(head, false)});
    trigger["verification_followup"] =
        json!({"stage":"checks_complete","proof":proof(head, false),"item_results":[]});
    request
}
/// The parent gate job's queue position and age.
pub fn queue_parent() -> Value {
    json!({"receipt":RECEIPT,"order":{RECEIPT:9},"metadata":{RECEIPT:{"at_ms":1_700_000_001_950_u64,"lane":"gate"}}})
}
fn pointer(receipt: &str) -> Value {
    json!({"receipt":receipt,"record":format!("private:{receipt}"),
        "digest":format!("sha256:{:x}", Sha256::digest(receipt.as_bytes()))})
}
/// A completed single verification round with one named risk per id.
pub fn completed_round(request: &Value, risks: &[&str]) -> Value {
    use crate::observer_review::gate_approval::followup::Binding;
    let binding = json!({"worker":request["worker"],"review":request["id"],"head":request["head"],"intent_revision":2});
    let id = serde_json::from_value::<Binding>(binding.clone())
        .unwrap()
        .id()
        .unwrap();
    let evidence = vec![pointer("ledger"), pointer("transcript")];
    let risks = risks.iter().map(|r| json!({"requirement_id":r,"unverified":format!("{r} is not confirmed by the checked evidence"),
        "acceptance_risk":format!("Acceptance leaves {r} unverified on this gate SHA."),
        "tried":[{"what":"Read the ledger and the delivered code.","receipt":"ledger"}],"evidence":[pointer("ledger")]})).collect::<Vec<_>>();
    json!({"id":id,"binding":binding,"stage":"complete","claimed":false,"evidence":evidence,
        "builder":{"request":pointer(&format!("{id}-builder")),"response":{"outcome":"evidence","response":pointer(&format!("{id}-builder-response"))}},
        "checks":{"receipt":pointer(&format!("{id}-checks")),"proof":proof(request["head"].as_str().unwrap(), false),"risks":risks,"verified":[]}})
}
/// Intent and drift stay semantic risks; plan and preservation are linked
/// non-executable obligations; normalize is executable.
fn admission_requirements(h: &str) -> Vec<Value> {
    vec![
        obligation(
            semantic(
                h,
                "req-intent",
                "the delivered phase matches the goal",
                "intent",
                &[],
            ),
            "non_executable",
        ),
        obligation(
            semantic(
                h,
                "req-drift",
                "the work did not drift from the goal",
                "drift",
                &[],
            ),
            "non_executable",
        ),
        obligation(
            semantic(
                h,
                "req-plan",
                "PLAN.md explains the completed scope",
                "intent",
                &[],
            ),
            "non_executable",
        ),
        obligation(
            semantic(
                h,
                "req-preserve",
                "existing guidance is preserved",
                "drift",
                &[],
            ),
            "non_executable",
        ),
        obligation(
            semantic(
                h,
                "req-normalize",
                "normalize rounds to two decimals",
                "intent",
                &["Math.round(n * 100) / 100"],
            ),
            "executable",
        ),
        no_push(h, "req-nopush"),
    ]
}
fn admission_report(head: &str) -> Value {
    let h = head;
    let risk = |why: &str| json!({"kind":"semantic_risk","why_not_executable":why});
    let requirements = admission_requirements(h);
    let contracts = vec![
        contract(
            "req-intent",
            "the delivered phase matches the goal",
            risk("Intent is a decision, not an executable contract."),
        ),
        contract(
            "req-drift",
            "no drift from current authority",
            risk("Drift compares work with authority."),
        ),
        contract(
            "req-normalize",
            "normalize rounds to two decimals",
            json!({"kind":"existing_check","test_index":0}),
        ),
    ];
    report(requirements, contracts)
}
pub fn admission(worker: &str, head: &str) -> Value {
    let h = head;
    let coverage = |id: &str| json!({"claim":id,"covered":false,"semantic_risk_admissible":false});
    let needs_check = ["req-intent", "req-drift", "req-plan", "req-preserve"]
        .iter()
        .map(|id| json!({"id":id,"claim":id,"coverage":null,"receipt":null,"tried":[]}))
        .collect::<Vec<_>>();
    let request = followup_request(worker, h);
    let round = completed_round(&request, &["req-intent", "req-drift"]);
    json!({"request":request,"report":admission_report(h),"round":round,"queue":queue_parent(),
        "assessment":{"passed":null,"state":"checkpoint_needs_thinker","violations":[],"needs_user":[],"needs_check":[],
            "needs_thinker":["req-intent","req-drift"],"unproven":["req-intent","req-drift"],
            "contract_coverage":{"req-intent":coverage("req-intent"),"req-drift":coverage("req-drift")}},
        "obligation_assessment":{"passed":false,"state":"checkpoint_needs_check","violations":[],"needs_user":[],
            "needs_thinker":["req-normalize","req-plan","req-preserve","req-intent","req-drift"],
            "unproven":["req-drift","req-intent","req-normalize","req-plan","req-preserve"],
            "needs_check":needs_check,
            "contract_coverage":{"req-normalize":coverage("req-normalize")}}})
}
/// A commit-and-tag requirement checked only by trusted controller receipts.
pub fn controller(worker: &str, head: &str) -> Value {
    let h = head;
    let docs = documents(h);
    let git = docs[6]["text"].as_str().unwrap().to_owned();
    let name = format!("the commit is tagged {TAG} and the result is reported");
    let mut tag = semantic(h, "req-commit-tag", &name, "intent", &[]);
    tag["citations"] = json!([
        cite(&docs[0], GOAL),
        cite(&docs[6], &git),
        cite(&docs[5], EVENT)
    ]);
    tag["applicability"] = applicability(&[2], false);
    tag["judgment"]["evidence_citations"] = json!([0, 1]);
    let tag = obligation(tag, "non_executable");
    let facts = json!({"kind":"controller_receipt","facts":[{"property":"head_sha"},{"property":"tag_head","tag":TAG}]});
    let contracts = vec![contract(
        "req-commit-tag",
        "the tag targets the delivered head",
        facts,
    )];
    let requirements = vec![tag, no_push(h, "req-nopush")];
    json!({"request":followup_request(worker, h),"report":report(requirements, contracts),
        "judgment":{"backend":"jev","model":"recorded-transport-stub","answers":{},"metrics":{},"fallback_reason":null}})
}
/// A deterministic repository: fixed identity, dates and content, so the
/// same files always give the same commit ids. Returns HEAD.
pub fn repository(path: &Path, commits: &[&[(&str, &str)]], tag: Option<&str>) -> String {
    std::fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "--template="]);
    for (n, files) in commits.iter().enumerate() {
        for (name, text) in *files {
            std::fs::write(path.join(name), text).unwrap();
        }
        git(path, &["add", "-A"]);
        git(path, &["commit", "-qm", &format!("Commit {}", n + 1)]);
    }
    if let Some(tag) = tag {
        git(path, &["tag", tag]);
    }
    git(path, &["rev-parse", "HEAD"])
}
pub fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_DATE", "1700000000 +0000")
        .env("GIT_COMMITTER_DATE", "1700000000 +0000")
        .args(["-c", "core.hooksPath=/dev/null", "-c", "user.name=Fixture"])
        .args([
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "init.defaultBranch=main",
            "-C",
        ])
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
/// The settings formatter as seven tracked files.
pub fn project_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("AGENTS.md", "# Agents\nKeep commits small.\n"),
        ("CLAUDE.md", "See AGENTS.md.\n"),
        ("PLAN.md", PLAN),
        (
            "normalize.cjs",
            "module.exports = (values) => values.map((n) => Math.round(n * 100) / 100);\n",
        ),
        (
            "serialize.cjs",
            "module.exports = (values) => JSON.stringify(values);\n",
        ),
        (
            "pipeline.cjs",
            "module.exports = (v) => require('./serialize.cjs')(require('./normalize.cjs')(v));\n",
        ),
        (
            "verification.cjs",
            "require('node:assert/strict').equal(require('./pipeline.cjs')([1.234]), '[1.23]');\nconsole.log('verification ok');\n",
        ),
    ]
}
/// Two commits with the gate tag on HEAD. Returns HEAD.
pub fn tagged_repository(path: &Path) -> String {
    let files = project_files();
    let second = [("PLAN.md", "# Plan\nPhase 1 is complete.\n")];
    repository(path, &[&files, &second], Some(TAG))
}
/// The tagged repository as a bundle of HEAD and the tag. Returns HEAD.
pub fn tagged_bundle(path: &Path, bundle: &Path) -> String {
    let head = tagged_repository(path);
    let tag = format!("refs/tags/{TAG}");
    git(
        path,
        &["bundle", "create", bundle.to_str().unwrap(), "HEAD", &tag],
    );
    head
}
