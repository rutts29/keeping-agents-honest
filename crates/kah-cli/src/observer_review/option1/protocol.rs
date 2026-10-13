//! Small native protocol: source atoms, whole-item selection, or checked rows.
use super::super::state::Request;
use crate::Result;
use kah_observer::{
    commit_check::Row,
    rule_source::{
        RuleList, Source,
        atomization::{AtomicRule, Item},
    },
    whole_preparation::Manifest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
/// Criterion for identifying checks that use only a code change.
pub const CHECKABILITY: &str = "a rule is diff-checkable when the code change alone can break it. Runtime measurements, investigations, sequencing and outside-design references are not diff-checkable";
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
    let report = decode(&request.id, bytes)?;
    check(&request.trigger["option1"], &report)?;
    Ok(report)
}
/// Parses a native report and binds it to its request before any job is read.
pub fn decode(request: &str, bytes: &[u8]) -> Result<Report> {
    let report: Report = serde_json::from_slice(bytes)?;
    if report.request != request {
        return Err("COMMIT_REPORT_BINDING stage=report".into());
    }
    Ok(report)
}
/// Checks a report against the complete job or the job parts selected for it.
pub fn check(job: &Value, report: &Report) -> Result<()> {
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
            let scope = super::report_scope::RowScope::read(&job["input"], rows)?;
            super::report_scope::validate_rows(&scope, rows)?;
        }
        (Answer::Selection { .. }, Some("selection")) => {}
        _ => return Err("COMMIT_REPORT_STAGE stage=report".into()),
    }
    Ok(())
}
pub fn schema(stage: &str) -> Value {
    let mut value = serde_json::json!({"protocol":"literal_first_option1","request":"copy request id","result":{"stage":stage},"typed_report":"kah_cli::observer_review::option1::protocol::Report"});
    if stage == "current_rules" {
        value["checkability_criterion"] = CHECKABILITY.into();
        value["result"]["candidate"] = serde_json::json!({
            "schema_version":1,"input_digest":"copy option1.input_digest","previous_digest":"copy rebuild.previous_digest (null on first set)",
            "rules":[{"id":"stable rule id","predicate":"literal whole rule","scope":"explicit scope","diff_checkable":true,"clauses":["clause id"],"permissions":[{"source":"source id","start":"UTF-8 byte offset","end":"UTF-8 byte offset","quote":"exact exception words"}]}],
            "clauses":[{"id":"unique clause id","words":{"source":"source id","start":"UTF-8 byte offset","end":"UTF-8 byte offset","quote":"exact source words"},"disposition":"contributes|merged|superseded|expired_under_condition|withdrawn|context|rejected_junk|unresolved_conflict","reason":"source-grounded reason","rules":["linked rule id"],"witnesses":[]}]
        });
    }
    value
}
