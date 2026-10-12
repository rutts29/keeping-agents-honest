//! Existing Jev-only receipts are visible even without a native review request.
use super::*;
pub(super) fn read(
    root: &Path,
    repo: &Path,
    cursor: &mut Cursor,
    gaps: &mut Vec<Value>,
) -> Vec<Value> {
    let directory = root.join("commit-reviews");
    if !directory.exists() {
        return vec![];
    }
    let files = match paths(&directory) {
        Ok(files) => files,
        Err(error) => {
            gaps.push(json!({"stage":"commit_journal","error":error.to_string()}));
            return vec![];
        }
    };
    files
        .into_iter()
        .filter_map(|path| match row(&path, root, repo, cursor, gaps) {
            Ok(row) => row,
            Err(error) => {
                gaps.push(json!({"path":path,"error":error.to_string()}));
                None
            }
        })
        .collect()
}
fn row(
    path: &Path,
    root: &Path,
    repo: &Path,
    cursor: &mut Cursor,
    gaps: &mut Vec<Value>,
) -> Result<Option<Value>> {
    let record = cursor.select(
        path,
        &Select::fields(&[
            ("head", Select::All),
            ("worker", Select::All),
            ("woke", Select::All),
            ("gaps", Select::Each(Box::new(Select::fields(&[])))),
            ("native", Select::fields(&[("review", Select::All)])),
            (
                "answers",
                Select::Each(Box::new(Select::fields(&[("request", Select::All)]))),
            ),
        ]),
    )?;
    if record["woke"] != false
        || !record["native"].is_null()
        || !record["gaps"].as_array().is_some_and(Vec::is_empty)
    {
        return Ok(None);
    }
    let head = record["head"]
        .as_str()
        .ok_or("KEEP_UP_COMMIT_HEAD stage=sample")?;
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("KEEP_UP_COMMIT_HEAD stage=sample".into());
    }
    let id = format!("jev-only-{head}");
    let timing = cursor.optional(
        &root.join("stage-timings").join(format!("{id}.json")),
        &["at_ms"],
        gaps,
    );
    let judged = timing["at_ms"]["judged"].as_u64();
    Ok(Some(json!({"id":id,"head":head,"purpose":"commit",
        "review_class":"commit","verdict_path":"jev_only","receipt":path,"worker":record["worker"],
        "answer_receipts":record["answers"],"commit_ms":commit_ms(repo,head,&mut cursor.commits),
        "commit_clock":"git committer timestamp, second precision","at_ms":{"judged":judged},
        "missing_stages":if judged.is_some(){json!([])}else{json!(["judged"])},
        "stage_sources":{"judged":if judged.is_some(){"per-commit persisted Jev-only verdict receipt"}else{"missing host judgment receipt"}},
        "assessment":{"state":"jev_only_recorded"}})))
}
