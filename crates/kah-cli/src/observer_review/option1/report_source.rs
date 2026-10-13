//! Observer report validation selects only the job parts its validator reads.
//! Documents, native history and the job's second (text) reference stay unread.
use super::protocol::{Answer, Report};
use crate::{
    Result,
    observer_review::source_store::{Select, read_selected},
};
use kah_observer::commit_check::Row;
use serde_json::Value;
use std::path::Path;
pub struct Header {
    pub id: String,
    pub option1: bool,
}
/// Request identity and job presence, without reading the job itself.
pub fn header(request: &Path) -> Result<Header> {
    let selected = read_selected(
        request,
        &Select::fields(&[
            ("id", Select::All),
            ("trigger", Select::fields(&[("option1", Select::Raw)])),
        ]),
    )?;
    let id = selected["id"]
        .as_str()
        .ok_or("REVIEW_REQUEST_JSON stage=report")?;
    // Storage references a job object as one JSON source; text never holds a job.
    let job = &selected["trigger"]["option1"];
    Ok(Header {
        id: id.into(),
        option1: job.is_object() && (job.get("$kah_source").is_none() || job["encoding"] == "json"),
    })
}
/// The stage plus exactly the job fields this report's validator checks.
pub fn job(request: &Path, report: &Report) -> Result<Value> {
    let selection = Select::fields(&[(
        "trigger",
        Select::fields(&[("option1", needs(&report.result))]),
    )]);
    let mut selected = read_selected(request, &selection)?;
    Ok(selected
        .get_mut("trigger")
        .and_then(|trigger| trigger.get_mut("option1"))
        .map(Value::take)
        .unwrap_or_default())
}
fn needs(answer: &Answer) -> Select {
    let part = match answer {
        Answer::CurrentRules { .. } => Some(("rebuild", Select::All)),
        Answer::Rules { .. } => Some(("items", Select::All)),
        Answer::Confirmation { rows } | Answer::Checkpoint { rows } => {
            Some(("input", rows_input(rows)))
        }
        Answer::Selection { .. } => None,
    };
    let mut fields = vec![("stage", Select::All)];
    fields.extend(part);
    Select::fields(&fields)
}
/// All rule IDs with flags (no hunk memberships) and every cited file whole.
fn rows_input(rows: &[Row]) -> Select {
    let cited = rows
        .iter()
        .flat_map(|row| &row.quotes)
        .map(|quote| (quote.file.clone(), Select::All))
        .collect();
    let flags = Select::fields(&[("prohibition", Select::All)]);
    Select::fields(&[
        ("revision", Select::All),
        ("rules", Select::Values(Box::new(flags))),
        ("files", Select::Fields(cited)),
    ])
}
