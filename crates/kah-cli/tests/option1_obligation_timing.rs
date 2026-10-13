//! Prohibitions are judged on every commit; obligations on the delivered
//! change; an explicit user instruction governs a conflicting standing file.
use kah_cli::observer_review::{
    option1::{checkpoint_outcome, input, precedence, protocol, requests},
    state::{Purpose, Request},
};
use kah_core::{current_rules as cr, intent_history::*};
use kah_observer::{
    commit_check::{Input, Status},
    rule_source::{CurrentSet, Link, Resolved, Rule, Source},
};
use serde_json::{Value, json};

const CHANGELOG: &str = "Every change must add an entry to the root CHANGELOG.md.";
const VENDOR: &str = "Do not modify vendor/.";
fn user() -> SourceRef {
    SourceRef::new(
        SourceProvider::Codex,
        "s",
        "u1",
        SourceRole::User,
        "sha256:u",
    )
}
fn history(goal: &str) -> History {
    History::new(GoalRevision::new(1, goal, vec![user()])).unwrap()
}
fn rule(id: &str, words: &str, kind: cr::RuleKind) -> Rule {
    Rule {
        id: id.into(),
        predicate: words.into(),
        diff_checkable: true,
        scope: "this repository".into(),
        permissions: vec![],
        kind,
        sources: vec![Link {
            source_id: "standing:AGENTS.md".into(),
            quote: words.into(),
        }],
    }
}
fn rules() -> Resolved {
    Resolved {
        source_digest: "set".into(),
        observer_session: "same-native".into(),
        source_words: [(
            "standing:AGENTS.md".to_owned(),
            format!("{VENDOR} {CHANGELOG}"),
        )]
        .into(),
        history_digest: None,
        rules: vec![
            rule("r3", VENDOR, cr::RuleKind::Prohibition),
            rule("r7", CHANGELOG, cr::RuleKind::Obligation),
        ],
    }
}
fn context(path: &str) -> Value {
    json!({"head":"a".repeat(40),"baseline_tree":"b".repeat(40),"touched_files":[{"path":path,"deleted":false,"patch":"+fixed()\n","content":{"text":"fixed()\n"}}]})
}
fn build(path: &str, checkpoint: bool) -> Input {
    let h = history("Fix the bugs. Make one commit per bug, then one changelog commit.");
    input::build(&h, &rules(), &context(path), checkpoint).unwrap()
}
fn request(stage: &str, input: &Input) -> Request {
    Request {
        id: "review-timing".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some(input.revision.clone()),
        intent: Value::Null,
        trigger: json!({"option1":{"stage":stage,"input":input}}),
        documents: vec![],
        created_ms: 0,
    }
}
fn row(input: &Input, id: &str, status: &str) -> Value {
    let (path, file) = input.files.iter().next().unwrap();
    json!({"requirement":id,"finding":"judged","status":status,"kind":"behavior","quotes":[
        {"file":path,"first_line":1,"last_line":1,"text":"fixed()\n","digest":file.digest}]})
}
fn report(stage: &str, rows: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({"request":"review-timing","result":{"stage":stage,"rows":rows}}))
        .unwrap()
}

#[test]
fn a_unmet_obligation_on_an_intermediate_commit_is_pending_not_a_violation() {
    let commit = build("src/csv_export.py", false);
    assert!(!commit.rules.contains_key("r7"));
    assert!(!commit.rules.contains_key("goal"));
    assert_eq!(
        commit.carried["deferred_obligations"]["rules"],
        json!(["r7"])
    );
    let (_, map) = requests::build(&commit, None).unwrap();
    assert!(
        map["rules"]
            .as_object()
            .unwrap()
            .values()
            .all(|r| r["full_rule_id"] != "r7")
    );
    // The report scope has no r7 row, so no Violated r7 row can reach the reminder.
    let req = request("confirmation", &commit);
    let r7 = report(
        "confirmation",
        vec![
            row(&commit, "r3", "satisfied"),
            row(&commit, "r7", "violated"),
        ],
    );
    assert!(protocol::validate(&req, &r7).is_err());
    let ok = report("confirmation", vec![row(&commit, "r3", "satisfied")]);
    assert!(protocol::validate(&req, &ok).is_ok());
}
#[test]
fn b_the_same_obligation_unmet_at_completion_is_still_reported() {
    let delivered = build("src/csv_export.py", true);
    assert!(!delivered.rules["r7"].prohibition);
    assert!(delivered.carried.get("deferred_obligations").is_none());
    let mut rows: Vec<Value> = delivered
        .rules
        .keys()
        .map(|id| row(&delivered, id, "satisfied"))
        .collect();
    let r7 = rows.iter().position(|r| r["requirement"] == "r7").unwrap();
    rows[r7] = row(&delivered, "r7", "violated");
    let req = request("checkpoint", &delivered);
    let checked = protocol::validate(&req, &report("checkpoint", rows)).unwrap();
    let protocol::Answer::Checkpoint { rows } = checked.result else {
        panic!("checkpoint rows")
    };
    let outcome = checkpoint_outcome::assess(&json!(delivered), &rows);
    assert_eq!(outcome["passed"], false);
    assert_eq!(outcome["violations"][0]["requirement"], "r7");
}
#[test]
fn c_a_broken_prohibition_on_an_intermediate_commit_is_still_confirmed() {
    let commit = build("vendor/lib.py", false);
    let r3 = &commit.rules["r3"];
    assert!(r3.prohibition && r3.unconditional_prohibition);
    let req = request("confirmation", &commit);
    let checked = protocol::validate(
        &req,
        &report("confirmation", vec![row(&commit, "r3", "violated")]),
    )
    .unwrap();
    let protocol::Answer::Confirmation { rows } = checked.result else {
        panic!("confirmation rows")
    };
    // advance::correction reminds the worker for every Violated row.
    assert_eq!(rows[0].status, Status::Violated);
}

fn conflict_input() -> cr::Input {
    let h = history("Fix the bug. Do not edit the root CHANGELOG.md.");
    let standing = cr::Source::standing("AGENTS.md", CHANGELOG, json!({"scope":"project"}));
    cr::Input::from_history(&h, vec![standing], "now").unwrap()
}
fn whole(s: &cr::Source) -> cr::Words {
    cr::Words {
        source: s.id.clone(),
        start: 0,
        end: s.text.len(),
        quote: s.text.clone(),
    }
}
fn clause(
    s: &cr::Source,
    d: cr::Disposition,
    rules: Vec<String>,
    witness: Vec<cr::Words>,
) -> cr::Clause {
    cr::Clause {
        id: s.id.clone(),
        words: whole(s),
        disposition: d,
        reason: "Explicit user instruction governs over the standing file for this scope".into(),
        rules,
        witnesses: witness,
    }
}
fn rebuild_request(input: &cr::Input) -> Request {
    let rebuild = json!({"input":input,"previous":null,"previous_digest":null,"direct_observer_review":false});
    Request {
        id: "rebuild".into(),
        worker: "w".into(),
        purpose: Purpose::Steering,
        turn: "t".into(),
        head: None,
        intent: Value::Null,
        trigger: json!({"option1":{"stage":"current_rules","rebuild":rebuild}}),
        documents: vec![],
        created_ms: 0,
    }
}
fn submit(req: &Request, set: &cr::RuleSet) -> Result<protocol::Report, String> {
    let body = json!({"request":req.id,"result":{"stage":"current_rules","candidate":set}});
    protocol::validate(req, &serde_json::to_vec(&body).unwrap()).map_err(|e| e.to_string())
}
/// The governed form: the user words are the rule; the standing clause is superseded.
fn governed(asked: &mut cr::RuleSet, goal: &cr::Source, file: &cr::Source) {
    asked.clauses[0] = clause(
        goal,
        cr::Disposition::Contributes,
        vec!["u1".into()],
        vec![],
    );
    asked.clauses[1] = clause(file, cr::Disposition::Superseded, vec![], vec![whole(goal)]);
    asked.rules = vec![cr::Rule {
        id: "u1".into(),
        predicate: goal.text.clone(),
        scope: "this task".into(),
        diff_checkable: true,
        kind: Some(cr::RuleKind::Prohibition),
        clauses: vec![goal.id.clone()],
        permissions: vec![],
    }];
}
#[test]
fn d_an_explicit_user_instruction_governs_a_conflicting_standing_rule() {
    let input = conflict_input();
    let (goal, file) = (&input.sources[0], &input.sources[1]);
    let req = rebuild_request(&input);
    let mut asked = cr::RuleSet {
        schema_version: 1,
        input_digest: input.digest().unwrap(),
        previous_digest: None,
        rules: vec![],
        clauses: vec![
            clause(
                goal,
                cr::Disposition::UnresolvedConflict,
                vec![],
                vec![whole(file)],
            ),
            clause(
                file,
                cr::Disposition::UnresolvedConflict,
                vec![],
                vec![whole(goal)],
            ),
        ],
    };
    assert!(precedence::user_governs(&asked.clauses[1], &input));
    let error = submit(&req, &asked).err().unwrap();
    assert!(error.contains("RULE_SET_PRECEDENCE"), "{error}");
    governed(&mut asked, goal, file);
    submit(&req, &asked).unwrap();
    let mut no_kind = asked.clone();
    no_kind.rules[0].kind = None;
    assert!(
        submit(&req, &no_kind)
            .err()
            .unwrap()
            .contains("RULE_SET_KIND")
    );
    let set = asked.validate(&input, None).unwrap();
    assert!(!set.has_conflicts()); // rule_questions::store asks nothing
    let resolved = CurrentSet::new(&set, &input, "same-native")
        .unwrap()
        .load(&[])
        .unwrap();
    assert_eq!(resolved.rules.len(), 1);
    assert!(
        resolved.rules[0]
            .sources
            .iter()
            .all(|l| l.source_id == goal.id)
    );
    // A conflict that the user words do not settle (standing against standing) can still ask.
    let mut standing_only = asked.clauses[1].clone();
    standing_only.witnesses = vec![whole(file)];
    assert!(!precedence::user_governs(&standing_only, &input));
}
