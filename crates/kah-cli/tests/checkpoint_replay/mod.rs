//! An authored checkpoint replay: a three-file project at one head, a tag
//! review whose nine non-diff rules stay unclear, and a same-head run end
//! with an empty range. Digests and quotes derive from the files here.
use kah_core::intent_history::{
    Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::commit_check::CodeFile;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub const HEAD: &str = "5b1e7c3a9d2f4e6b8a0c1d3e5f7a9b2c4d6e8f01";
pub const BASELINE: &str = "2c4e6a8b0d1f3e5a7c9b1d3f5e7a9c0b2d4f6e81";
pub const TAG_REQUEST: &str = "review-1700000300000-301";
pub const END_REQUEST: &str = "review-1700000400000-401";
pub const GATE_REQUEST: &str = "review-1700000200000-201";
pub const GOAL: &str =
    "Build a settings normalizer and serializer, check them, then tag the release.";
const ADDED: &str = "Never push or deploy; I will review and publish myself.";

const FILES: [(&str, &str); 3] = [
    (
        "PLAN.md",
        "# Plan\n\n1. Add normalize.cjs.\n2. Add serialize.cjs.\n3. Tag the release after checks pass.\n",
    ),
    (
        "normalize.cjs",
        "module.exports = function normalize(settings) {\n  return Object.fromEntries(Object.entries(settings).map(([k, v]) => [k.trim(), v]));\n};\n",
    ),
    (
        "serialize.cjs",
        "module.exports = function serialize(settings) {\n  return JSON.stringify(settings, Object.keys(settings).sort());\n};\n",
    ),
];

/// (id, prohibition, literal check). The first nine need run evidence.
const RULES: [(&str, bool, bool); 12] = [
    ("goal", false, false),
    ("no-push-deploy", true, false),
    ("no-force-push", true, false),
    ("commit-per-step", false, false),
    ("tag-after-checks", false, false),
    ("run-verification", false, false),
    ("no-new-dependencies", true, false),
    ("keep-plan-current", false, false),
    ("report-evidence", false, false),
    ("normalize-fn", false, true),
    ("serialize-fn", false, true),
    ("no-secrets", true, true),
];

fn files() -> Value {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), json!(CodeFile::new(HEAD, text))))
        .collect::<Map<_, _>>()
        .into()
}
fn rules() -> Value {
    RULES
        .iter()
        .map(|(id, prohibition, literal)| {
            let rule = json!({"authority":if *id=="goal"{"goal".to_string()}else{format!("derived_rule:{id}")},
                "hunks":[],"literal_check":literal,"prohibition":prohibition,"unconditional_prohibition":prohibition});
            (id.to_string(), rule)
        })
        .collect::<Map<_, _>>()
        .into()
}
fn input(code_changed: bool) -> Value {
    json!({"revision":HEAD,"baseline":if code_changed{BASELINE}else{HEAD},"checkpoint":true,
        "code_changed":code_changed,"files":if code_changed{files()}else{json!({})},"hunks":{},
        "impacted":RULES.iter().map(|r|r.0).collect::<Vec<_>>(),"matrix":[],
        "carried":{"intent_revision":2},"intent":{"goal":GOAL,"items":{"goal":{"origin":"user","text":GOAL}},"conflicts":[]},
        "rules":rules()})
}
fn quote(path: &str) -> Value {
    let (_, text) = FILES.iter().find(|(p, _)| *p == path).unwrap();
    json!({"file":path,"first_line":1,"last_line":text.lines().count(),"text":text,
        "digest":CodeFile::new(HEAD, text).digest})
}
fn tag_rows() -> Vec<Value> {
    RULES
        .iter()
        .map(|(id, _, literal)| match *id {
            "normalize-fn" | "serialize-fn" => {
                let file = format!("{}.cjs", id.trim_end_matches("-fn"));
                json!({"requirement":id,"status":"satisfied","kind":"behavior",
                    "finding":format!("{file} exports the required function."),"quotes":[quote(&file)]})
            }
            "no-secrets" => json!({"requirement":id,"status":"satisfied","kind":"not_violated",
                "finding":"No file in the change holds a credential.","quotes":[]}),
            _ => {
                assert!(!literal);
                json!({"requirement":id,"status":"unclear","kind":"behavior",
                    "finding":"The code alone cannot show this; it needs run evidence.","quotes":[]})
            }
        })
        .collect()
}
fn end_rows() -> Vec<Value> {
    RULES
        .iter()
        .map(|(id, _, _)| json!({"requirement":id,"status":"unclear","kind":"behavior",
            "finding":"This range has no changed files, so it adds no evidence for this rule.","quotes":[]}))
        .collect()
}
fn case(id: &str, input: Value, trigger: Value, rows: Vec<Value>) -> Value {
    json!({"id":id,"input":input,"trigger":trigger,
        "report":{"request":id,"result":{"stage":"checkpoint","rows":rows}},
        "latest":{"request":id,"head":HEAD,"purpose":"checkpoint",
            "assessment":{"state":"checkpoint_unconfirmed","passed":null},"rows":rows}})
}
pub fn tag_trigger() -> Value {
    json!({"events":[{"at_ms":1_700_000_250_000u64,"initial_sample":false,"kind":"native_git_tag",
        "source":"controller Git metadata","tags":[format!("refs/tags/release-1 {HEAD}")]}],
        "queued_at_ms":1_700_000_260_000u64,"review_class":"gate"})
}
pub fn completion_trigger() -> Value {
    let command = "node check.cjs";
    json!({"events":[{"actor":"kah controller; checks run on the agent's commit","intent_revision":2,"kind":"run_completion",
        "completion_key":{"action_digest":"sha256:5d1c0e7a","command":command,"commit":HEAD,
            "instance":"run-1700000000000-1","online":false,"user_sequence":0},
        "verification":{"command":command,"commit":HEAD,"elapsed_ms":1200,"exit_code":0,
            "output":"CHECK_PASS\n","output_truncated":false,"success":true}}],
        "queued_at_ms":1_700_000_360_000u64,"review_class":"gate"})
}
/// The tag review of the changed range.
pub fn tag() -> Value {
    case(TAG_REQUEST, input(true), tag_trigger(), tag_rows())
}
/// The run end on the same head: an empty range.
pub fn end() -> Value {
    case(END_REQUEST, input(false), completion_trigger(), end_rows())
}
/// Intent at revision 2: the goal and one later user constraint.
pub fn history() -> History {
    let source = |record: &str, text: &str| {
        SourceRef::new(
            SourceProvider::Claude,
            "worker-session",
            record,
            SourceRole::User,
            format!("sha256:{:x}", Sha256::digest(text)),
        )
    };
    let mut history =
        History::new(GoalRevision::new(1, GOAL, vec![source("user-1", GOAL)])).unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "no-push-deploy",
                IntentKind::Constraint,
                ADDED,
                vec![source("user-2", ADDED)],
            ),
            approval: None,
        })
        .unwrap();
    history
}
/// The captured gate request: the parent of a typed continuation.
pub fn gate_request(worker: &str) -> Value {
    json!({"id":GATE_REQUEST,"worker":worker,"head":HEAD,"purpose":"checkpoint","turn":"gate-turn",
        "created_ms":1_700_000_200_000u64,"documents":[],
        "intent":{"revision":2,"goal":GOAL,"goal_source":[],"items":[{"id":"no-push-deploy","text":ADDED,"source":[]}],
            "completed_outcomes":[],"failures":[],"open_deferrals":[],"open_outcomes":[]},
        "trigger":{"events":[{"at_ms":1_700_000_190_000u64,"kind":"user_gate","source":"host_cli"}],
            "queued_at_ms":1_700_000_195_000u64,"review_class":"gate"}})
}
