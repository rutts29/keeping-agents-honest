//! Small native protocol: source atoms, whole-item selection, or checked rows.
use super::super::state::Request;
use crate::Result;
use kah_observer::{
    commit_check::{Input, Row},
    rule_source::{
        RuleList, Source,
        atomization::{AtomicRule, Item},
    },
    whole_preparation::Manifest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub request: String,
    pub result: Answer,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    CurrentRules {
        candidate: kah_core::current_rules::RuleSet,
    },
    Rules {
        rules: Vec<AtomicRule>,
    },
    Confirmation {
        rows: Vec<Row>,
    },
    Selection {
        manifests: Vec<Manifest>,
    },
    Checkpoint {
        rows: Vec<Row>,
    },
}
pub fn active(request: &Request) -> bool {
    request.trigger["option1"].is_object()
}
pub fn validate(request: &Request, bytes: &[u8]) -> Result<Report> {
    let report: Report = serde_json::from_slice(bytes)?;
    if report.request != request.id {
        return Err("COMMIT_REPORT_BINDING stage=report".into());
    }
    let job = &request.trigger["option1"];
    match (&report.result, job["stage"].as_str()) {
        (Answer::CurrentRules { candidate }, Some("current_rules")) => {
            let rebuild: kah_core::run_store::RuleRebuild =
                serde_json::from_value(job["rebuild"].clone())?;
            candidate.validate_report(&rebuild.input, rebuild.previous.as_ref())?;
        }
        (Answer::Rules { rules }, Some("rules")) => {
            let items: Vec<Item> = serde_json::from_value(job["items"].clone())?;
            let list = RuleList {
                input_digest: kah_observer::context_selection::digest(&serde_json::to_value(
                    &items,
                )?),
                observer_session: "host-validated-on-consume".into(),
                rules: rules.clone(),
            };
            list.load(&items)?;
        }
        (Answer::Confirmation { rows }, Some("confirmation"))
        | (Answer::Checkpoint { rows }, Some("checkpoint")) => {
            let input: Input = serde_json::from_value(job["input"].clone())?;
            validate_rows(&input, rows)?;
        }
        (Answer::Selection { .. }, Some("selection")) => {}
        _ => return Err("COMMIT_REPORT_STAGE stage=report".into()),
    }
    Ok(report)
}
fn validate_rows(input: &Input, rows: &[Row]) -> Result<()> {
    let expected = input
        .rules
        .keys()
        .collect::<std::collections::BTreeSet<_>>();
    let actual = rows
        .iter()
        .map(|r| &r.requirement)
        .collect::<std::collections::BTreeSet<_>>();
    if expected != actual || rows.len() != actual.len() {
        return Err("COMMIT_REPORT_SCOPE stage=report".into());
    }
    for row in rows {
        if row.requirement == "controller_intent_pending"
            && row.status != kah_observer::commit_check::Status::Unclear
        {
            return Err(
                "COMMIT_INTENT_NOT_ACCEPTED stage=report: startup intent stays unconfirmed".into(),
            );
        }
        if row.kind == kah_observer::commit_check::FindingKind::NotViolated
            && !input.rules[&row.requirement].prohibition
        {
            return Err("COMMIT_REPORT_ABSENCE_SCOPE stage=report".into());
        }
        if row.status == kah_observer::commit_check::Status::Violated && row.quotes.is_empty() {
            return Err("COMMIT_REPORT_QUOTES stage=report".into());
        }
        for quote in &row.quotes {
            kah_observer::commit_check::verify_quote(input, quote)?;
        }
    }
    Ok(())
}
pub fn schema(stage: &str) -> Value {
    let mut value = serde_json::json!({"protocol":"literal_first_option1","request":"copy request id","result":{"stage":stage},"typed_report":"kah_cli::observer_review::option1::protocol::Report"});
    if stage == "current_rules" {
        value["result"]["candidate"] = serde_json::json!({
            "schema_version":1,"input_digest":"copy option1.input_digest","previous_digest":"copy rebuild.previous_digest (null on first set)",
            "rules":[{"id":"stable rule id","predicate":"literal whole rule","scope":"explicit scope","diff_checkable":true,"clauses":["clause id"],"permissions":[{"source":"source id","start":"UTF-8 byte offset","end":"UTF-8 byte offset","quote":"exact exception words"}]}],
            "clauses":[{"id":"unique clause id","words":{"source":"source id","start":"UTF-8 byte offset","end":"UTF-8 byte offset","quote":"exact source words"},"disposition":"contributes|merged|superseded|expired_under_condition|withdrawn|context|rejected_junk|unresolved_conflict","reason":"source-grounded reason","rules":["linked rule id"],"witnesses":[]}]
        });
    }
    value
}
