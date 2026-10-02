use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::{engine, session::Store};
use kah_run::{store::Record, verification::Receipt, worktree::Worktree};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn write(
    directory: &Path,
    id: &str,
    record: &Record,
    tree: &Worktree,
    tests: Option<&Receipt>,
    problem: Option<&str>,
) -> Result<()> {
    let session = record
        .native
        .as_ref()
        .and_then(|n| n.session.as_deref())
        .unwrap_or(id);
    let observer = Store::read(&RunStore::default_state_root()?, session)?;
    let projection = observer
        .as_ref()
        .map(engine::projection)
        .transpose()?
        .flatten();
    let goal = projection
        .as_ref()
        .map(|p| p.goal.text.as_str())
        .unwrap_or(record.control.task());
    let report = json!({
        "run":id,"goal":goal,"result":record.control.status(),"branch":format!("kah/{id}"),
        "native":record.native.as_ref().map(|n|json!({"provider":n.provider,"session":n.session,"name":n.name})),
        "sourceProject":record.project,"worktree":record.worktree,
        "checkpoint":record.control.last_checkpoint(),"deliveredCommit":record.control.last_checkpoint(),"diffStats":tree.diff_stat()?,
        "tests":tests.filter(|t| Some(t.commit.as_str())==record.control.last_checkpoint()),"openQuestion":record.control.external_question(),"error":problem,
        "ledgerSummary":projection.map(|p|p.rendered_prompt()),"observerGap":observer.and_then(|s|s.gap),
        "tokensReported":record.control.tokens_used(),"elapsedMs":record.control.elapsed_ms(),
        "handoff":"Review the local branch diff and merge it yourself. Only the agent's captured HEAD is delivered; uncommitted files remain in the worktree. kah did not author a commit, push or deploy."
    });
    save(directory, "report.json", &report)?;
    eprintln!(
        "\x07kah {id}: {:?}. Report: {}",
        record.control.status(),
        directory.join("report.json").display()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|e| format!("RUN_REPORT_JSON stage=report: {e}"))?
    );
    Ok(())
}
pub fn save(directory: &Path, name: &str, value: &Value) -> Result<()> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("RUN_REPORT_CLOCK stage=report: {e}"))?
        .as_nanos();
    let temporary = directory.join(format!("report-{nonce}.tmp"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(io_error)?;
    file.write_all(
        &serde_json::to_vec_pretty(value)
            .map_err(|e| format!("RUN_REPORT_JSON stage=report: {e}"))?,
    )
    .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(temporary, directory.join(name)).map_err(io_error)?;
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    Ok(())
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_REPORT_IO stage=report: {error}")
}
