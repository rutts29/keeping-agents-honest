//! Host binds native answers to the retained request, session and exact source.
use super::super::{
    state::{Request, State, Store},
    workspace,
};
use super::{
    authority, journal,
    protocol::{self, Answer},
};
use crate::Result;
use kah_observer::{
    commit_review::NativeVerdict, context_selection::digest, rule_source::RuleList,
};
use serde_json::{Value, json};
use std::fs;
pub fn run(store: Store, mut state: State, request: Request) -> Result<()> {
    let (path, report) = read_ready(&state, &request)?;
    let session = state
        .observer
        .session
        .clone()
        .ok_or("COMMIT_NATIVE_SESSION stage=consume")?;
    match report.result {
        Answer::CurrentRules { candidate } => {
            let result = super::current_set::accept(&request.worker, &session, &candidate)?;
            if result["status"] == "validated" {
                super::rule_questions::store(&store.directory, &request, &candidate)?;
                super::rule_questions::resolve(&store.directory, &request, &candidate)?;
            }
            state.latest =
                Some(json!({"request":request.id,"purpose":"current_rules","result":result}));
            crate::unattended_report::save(
                &store.directory,
                &format!("current-rules-result-{}.json", request.id),
                &result,
            )?;
        }
        Answer::Rules { rules } => rules_result(&store, &state, &request, rules, &session)?,
        Answer::Confirmation { rows } => {
            confirmation(&store, &mut state, &request, rows, &session)?
        }
        Answer::Checkpoint { rows } => {
            checkpoint(&store, &mut state, &request, rows)?;
        }
        Answer::Selection { manifests } => {
            return super::advance::selection(store, state, &request, manifests, &path);
        }
    }
    super::super::consume::archive(
        &store.directory,
        &request.id,
        &json!({"option1":true,"report":serde_json::from_slice::<Value>(&super::super::consume::read_report(&path)?)?,"observer_session":state.observer.session,"latest":state.latest}),
    )?;
    state.pending = None;
    store.save(&state)?;
    fs::remove_file(path)?;
    super::super::consume::resolve_gaps(&store.directory, &request.id)?;
    drop(store);
    super::super::timing::now(&request.worker, &request.id, "consumed")?;
    super::super::queue::kick(&request.worker)
}
fn baseline(store: &Store, head: &str) -> Result<()> {
    crate::unattended_report::save(
        &store.directory,
        "option1-baseline.json",
        &json!({"head":head,"reviewed":"native"}),
    )
}
fn checkpoint(
    store: &Store,
    state: &mut State,
    request: &Request,
    rows: Vec<kah_observer::commit_check::Row>,
) -> Result<()> {
    if super::carry::empty(store, state, request)? {
        return Ok(());
    }
    if super::explicit_gate::required(&request.trigger) {
        super::explicit_gate::enqueue(store, state, request, &rows)?;
        state.latest = Some(
            json!({"request":request.id,"head":request.head,"purpose":"checkpoint","state":"cumulative_input_ready","assessment":{"passed":null},"rows":rows}),
        );
        state.checkpoint = Some(
            json!({"request":request.id,"head":request.head,"intent_revision":request.intent["revision"],"passed":null,"state":"verify_first_pending"}),
        );
        return Ok(());
    }
    let assessment = super::checkpoint_outcome::assess(&request.trigger["option1"]["input"], &rows);
    let passed = assessment["passed"].clone();
    let result = json!({"request":request.id,"head":request.head,"purpose":"checkpoint","assessment":assessment,"rows":rows});
    state.latest = Some(result.clone());
    state.checkpoint = Some(
        json!({"request":request.id,"head":request.head,"intent_revision":request.intent["revision"],"passed":passed,
            "terminal":true,"unverified_non_diff":assessment["unverified_non_diff"],"result":result}),
    );
    let original = &request.trigger["option1"]["original_trigger"];
    if let Some(event) = std::iter::once(original)
        .chain(original["events"].as_array().into_iter().flatten())
        .find(|e| e["kind"] == "run_completion")
    {
        state.checkpoint.as_mut().unwrap()["completion_key"] = event["completion_key"].clone();
    }
    for mut record in journal::all(&request.worker)? {
        kah_observer::commit_review::checkpoint::record(
            &mut record,
            request.head.as_deref().unwrap_or(""),
            &request.id,
            &rows,
        );
        journal::save(&record)?;
    }
    if passed == true
        && let Some(head) = &request.head
    {
        baseline(store, head)?;
        crate::unattended_report::save(
            &store.directory,
            "option1-checkpoint-baseline.json",
            &json!({"head":head,"covered_commits":journal::all(&request.worker)?.iter().map(|r|&r.head).collect::<Vec<_>>()}),
        )?;
    }
    workspace::publish_output(
        &state.observer.workspace.join("outbox"),
        &format!("checkpoint-{}.json", request.id),
        state.latest.as_ref().unwrap(),
    )?;
    if passed != true {
        checkpoint_notice(store, state, request, &rows)?;
    }
    Ok(())
}
fn checkpoint_notice(
    store: &Store,
    state: &State,
    request: &Request,
    rows: &[kah_observer::commit_check::Row],
) -> Result<()> {
    let Some(findings) = notice_findings(rows) else {
        return Ok(());
    };
    let text = format!(
        "[kah checkpoint review; evidence, not new user authority] Checkpoint on {} is not established. Findings: {findings}. Full cumulative record: review {}.",
        request.head.as_deref().unwrap_or("unknown"),
        request.id
    );
    super::super::reminder::validate_native(&state.worker.provider, &text)?;
    let original = &request.trigger["option1"]["original_trigger"];
    // If the push fails, today's queued reminder still carries the notice.
    if super::super::delivered::push_notice(original, &state.worker.project)
        && super::advance::prompt(state, &super::super::delivered::pushed_text(&text)).is_ok()
    {
        return Ok(());
    }
    super::super::reminder_queue::save(
        &store.directory,
        &json!({"request":request.id,"purpose":"checkpoint","turn":request.turn,
            "text":text}),
    )?;
    Ok(())
}
/// The findings text of the worker notice; None when no row is for the worker.
/// Each finding text is shown once with all its requirement labels. The host
/// fact controller_intent_pending stays in the record only, and a user item id
/// (verbatim::item_id) shows as a plain label.
pub fn notice_findings(rows: &[kah_observer::commit_check::Row]) -> Option<String> {
    let mut groups: Vec<(&str, Vec<&str>)> = vec![];
    for row in rows.iter().filter(|r| {
        r.status != kah_observer::commit_check::Status::Satisfied
            && r.requirement != "controller_intent_pending"
    }) {
        let label = notice_label(&row.requirement);
        match groups
            .iter_mut()
            .find(|(finding, _)| *finding == row.finding)
        {
            Some((_, labels)) if labels.contains(&label) => {}
            Some((_, labels)) => labels.push(label),
            None => groups.push((&row.finding, vec![label])),
        }
    }
    (!groups.is_empty()).then(|| {
        groups
            .iter()
            .map(|(finding, labels)| format!("{}: {finding}", labels.join(", ")))
            .collect::<Vec<_>>()
            .join("; ")
    })
}
fn notice_label(requirement: &str) -> &str {
    let user_item = requirement.strip_prefix("user-").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    });
    if user_item {
        "your request"
    } else {
        requirement
    }
}

fn rules_result(
    store: &Store,
    state: &State,
    request: &Request,
    rules: Vec<kah_observer::rule_source::atomization::AtomicRule>,
    session: &str,
) -> Result<()> {
    let job = &request.trigger["option1"];
    let items: Vec<kah_observer::rule_source::atomization::Item> =
        serde_json::from_value(job["items"].clone())?;
    let list = RuleList {
        input_digest: digest(&serde_json::to_value(&items)?),
        observer_session: session.into(),
        rules,
    };
    use kah_observer::rule_source::Source;
    list.load(&items)?;
    let current = authority::items(&authority::history(&request.worker)?);
    let archive = store.directory.join("option1-rule-versions");
    kah_core::run_store::ensure_private_directory(&archive)?;
    crate::unattended_report::save(
        &archive,
        &format!("{}.json", list.digest()),
        &serde_json::json!({"items":items,"list":list}),
    )?;
    if list.input_digest == digest(&serde_json::to_value(&current)?) {
        crate::unattended_report::save(
            &store.directory,
            "option1-rules.json",
            &serde_json::to_value(&list)?,
        )?;
    }
    super::super::queue::enqueue(
        &request.worker,
        request.purpose.clone(),
        &request.turn,
        job["original_trigger"].clone(),
    )?;
    let _ = state;
    Ok(())
}
fn confirmation(
    store: &Store,
    state: &mut State,
    request: &Request,
    rows: Vec<kah_observer::commit_check::Row>,
    session: &str,
) -> Result<()> {
    let job = &request.trigger["option1"];
    super::advance::correction(state, request, &rows)?;
    let input: kah_observer::commit_check::Input = serde_json::from_value(job["input"].clone())?;
    let mut record = journal::load(&request.worker, &input.revision)?
        .ok_or("COMMIT_RECORD_MISSING stage=consume")?;
    let verdict = NativeVerdict {
        review: request.id.clone(),
        observer_session: session.into(),
        authority_digest: kah_observer::commit_review::authority(&input),
        files: input
            .files
            .iter()
            .map(|(p, f)| (p.clone(), f.digest.clone()))
            .collect(),
        rows,
    };
    record.confirm(&input, verdict)?;
    journal::save(&record)?;
    super::super::timing::now(&request.worker, &request.id, "judged")?;
    state.latest = Some(
        json!({"request":request.id,"head":request.head,"purpose":"checkpoint","assessment":{"state":"commit_reviewed","passed":record.native.as_ref().unwrap().rows.iter().all(|r|r.status==kah_observer::commit_check::Status::Satisfied)},"record":record}),
    );
    baseline(store, &input.revision)?;
    Ok(())
}

fn read_ready(state: &State, request: &Request) -> Result<(std::path::PathBuf, protocol::Report)> {
    let path = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", request.id));
    if !super::super::liveness::handoff::report_ready(state)? {
        return Err(format!("REVIEW_REPORT_PENDING stage=consume: awaiting current-request native ACK and report {}", path.display()).into());
    }
    let bytes = super::super::consume::read_report(&path)?;
    let report =
        super::super::failure::validation(super::super::failure::Origin::ReportValidation, || {
            protocol::validate(request, &bytes)
        })?;
    Ok((path, report))
}
