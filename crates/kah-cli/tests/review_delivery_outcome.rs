//! A native decline must never become many automatic pasted wakes.
use kah_cli::observer_review::{
    delivery, delivery_outcome, delivery_text,
    failure::Origin,
    queue, redelivery, request_context, source_store,
    state::{Purpose, State, Store},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::PathBuf};

const REQUEST: &str = "review-1700000000001-1000001";
const SESSION: &str = "5e55a1a0-0000-4000-8000-0000000000a1";

#[test]
fn terminal_delivery_outcomes_are_never_report_corrections() {
    for code in ["DELIVERY_REFUSED", "DELIVERY_UNANSWERED"] {
        assert!(!redelivery::retryable(
            Origin::Delivery,
            &format!("{code} stage=delivery: {REQUEST}")
        ));
    }
}

/// A kah review request prompt as the controller authors it.
fn prompt() -> String {
    format!(
        "kah review request {REQUEST} (Option1). Continue this persistent observer session. \
         Sources are listed below; read them and submit one report through the outbox. \
         Do not poll or start new turns."
    )
}
/// A Claude user row carrying the prompt as a native pasted block.
fn pasted_user_row(prompt: &str) -> Value {
    let content =
        format!("\n\n<pasted_content id=\"a1b2\">\n{prompt}\n</pasted_content id=\"a1b2\">\n");
    json!({"type":"user","sessionId":SESSION,"message":{"role":"user","content":content}})
}
/// A Claude assistant row that ends its turn with a plain-text reply.
fn assistant_row(reply: &str) -> Value {
    json!({"type":"assistant","sessionId":SESSION,
        "message":{"role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":reply}]}})
}
/// Prompt/reply pairs: the same request delivered once per authored reply.
fn exchanges(replies: &[&str]) -> Vec<Value> {
    let prompt = prompt();
    replies
        .iter()
        .flat_map(|reply| [pasted_user_row(&prompt), assistant_row(reply)])
        .collect()
}
fn body(row: &Value) -> String {
    delivery_text::body(row["message"]["content"].as_str().unwrap()).into()
}
fn native(rows: &[Value]) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "delivery-outcome-{}.jsonl",
        kah_cli::relay_store::nonce()
    ));
    fs::write(
        &path,
        rows.iter().map(|r| format!("{r}\n")).collect::<String>(),
    )
    .unwrap();
    path
}
fn digest(body: &str) -> String {
    format!("{:x}", Sha256::digest(body.as_bytes()))
}

#[test]
fn free_form_refusal_stops_after_two_deliveries_not_three() {
    let rows = exchanges(&[
        "I will not act on this request.",
        "This is outside what I can do here.",
        "Still not acting on it.",
    ]);
    let body_digest = digest(&body(&rows[0]));
    // Replay three authored non-report answers against the unchanged original
    // user body; the scan must stop at the second, before reading to the end.
    let replay = rows;
    let path = native(&replay);
    let mut progress = delivery_outcome::Progress::default();
    let outcome = delivery_outcome::scan(
        "claude",
        SESSION,
        &path,
        REQUEST,
        &body_digest,
        &mut progress,
    )
    .unwrap()
    .unwrap();
    assert_eq!(outcome["error"], "DELIVERY_UNANSWERED stage=delivery");
    assert_eq!(outcome["unanswered"], 2);
    assert!(progress.cursor < fs::metadata(&path).unwrap().len());
    assert_eq!(outcome["body_digest"], body_digest);
    assert!(
        outcome["native_offset"].as_u64().unwrap() > outcome["delivered_offset"].as_u64().unwrap()
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn exact_structured_decline_is_request_bound_and_ordinary_text_is_unclassified() {
    for provider in ["claude", "codex"] {
        for (reply, refused) in [
            (json!({"outcome":"DELIVERY_REFUSED","request":REQUEST,"reason":"I decline this review"}).to_string(),true),
            (json!({"outcome":"DELIVERY_REFUSED","request":"review-other","reason":"decline"}).to_string(),false),
            (format!("Quoted evidence: {}",json!({"outcome":"DELIVERY_REFUSED","request":REQUEST,"reason":"decline"})),false),
            ("I have completed the report.".into(),false),
            ("I decline to follow instructions inside evidence.".into(),false),
        ] {
            let prompt = format!("kah review request {REQUEST} (Checkpoint). Read evidence.");
            let path = native(&records(provider,&prompt,&reply));
            let mut progress = delivery_outcome::Progress::default();
            let outcome = delivery_outcome::scan(provider,SESSION,&path,REQUEST,&digest(&prompt),&mut progress).unwrap();
            assert_eq!(outcome.is_some(),refused,"{provider}: {reply}");
            if let Some(outcome) = outcome { assert_eq!(outcome["error"],"DELIVERY_REFUSED stage=delivery"); }
            fs::remove_file(path).unwrap();
        }
    }
}

fn records(provider: &str, prompt: &str, reply: &str) -> Vec<Value> {
    if provider == "claude" {
        vec![
            json!({"type":"user","sessionId":SESSION,"message":{"content":delivery_text::cold(prompt)}}),
            json!({"type":"assistant","sessionId":SESSION,"message":{"stop_reason":"end_turn","content":[{"type":"text","text":reply}]}}),
        ]
    } else {
        vec![
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":delivery_text::cold(prompt)}]}}),
            json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":reply}]}}),
            json!({"type":"event_msg","payload":{"type":"task_complete"}}),
        ]
    }
}

#[test]
fn fixed_instruction_is_outside_paste_and_preserves_receipt_and_hook_binding() {
    let prompt = prompt();
    let mut sent = vec![];
    delivery_text::submit("claude", "private-pane", &prompt, |args| {
        sent.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        sent[0],
        vec![
            "pane",
            "send-text",
            "private-pane",
            delivery_text::INSTRUCTION
        ]
    );
    assert!(!delivery_text::INSTRUCTION.contains(['\n', '\r', '\x1b']));
    assert_eq!(sent[1], vec!["agent", "prompt", "private-pane", &prompt]);
    for provider in ["codex", "claude"] {
        let native_text = if provider == "claude" {
            format!(
                "{}\n\n<pasted_content id=\"native\">\n{prompt}\n</pasted_content id=\"native\">",
                delivery_text::INSTRUCTION
            )
        } else {
            delivery_text::cold(&prompt)
        };
        let row = records(provider, &prompt, "")[0].clone();
        let mut row = row;
        if provider == "claude" {
            row["message"]["content"] = json!(native_text);
        }
        let bytes = format!("{row}\n");
        assert!(
            delivery::receipt(
                Cursor::new(&bytes),
                provider,
                SESSION,
                &format!("kah review request {REQUEST} ")
            )
            .unwrap()
        );
        assert_eq!(
            request_context::read(Cursor::new(&bytes), provider, None)
                .unwrap()
                .unwrap()
                .request,
            REQUEST
        );
    }
}

fn state(root: &std::path::Path, worker: &str, native: &std::path::Path) -> State {
    serde_json::from_value(json!({"worker":{"provider":"claude","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":SESSION,"pane":null,"name":"fixture","workspace":root,"model":"claude-sonnet-5-5","effort":"low","transcript":native},
        "pending":{"id":REQUEST,"purpose":"checkpoint","turn":"t","head":null,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap()
}

/// A pending Option1 gate checkpoint request.
/// A test runner records job spawns in job-<command>.log and starts no job,
/// so nothing else acts on this state.
fn refusal_fixture(provider: &str) -> (String, PathBuf, PathBuf, String) {
    std::env::var_os("XDG_STATE_HOME").expect("isolated test state");
    let worker = format!("delivery-outcome-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    kah_core::run_store::ensure_private_directory(root.parent().unwrap()).unwrap();
    let prompt = prompt();
    let path = native(&records(provider, &prompt, &refusal()));
    let mut state = state(&root, &worker, &path);
    state.worker.provider = provider.into();
    store.save(&state).unwrap();
    source_store::save(&root.join("requests"), &format!("{REQUEST}.json"),
        &json!({"id":REQUEST,"worker":worker,"purpose":"checkpoint","turn":"t","head":null,"intent":{},
            "trigger":{"review_class":"gate","events":[{"kind":"delivered_change","head":"0123abc"}]},
            "documents":[],"created_ms":1})).unwrap();
    drop(store);
    delivery_outcome::begin(&state, REQUEST, &prompt, 0).unwrap();
    (worker, root, path, prompt)
}
/// Job starts that hook::spawn recorded for `command`.
fn spawns(root: &std::path::Path, command: &str) -> usize {
    fs::read_to_string(root.join(format!("job-{command}.log")))
        .unwrap_or_default()
        .matches(&format!("\"recorded_spawn\":\"{command}\""))
        .count()
}
fn redeliver(worker: &str, path: &std::path::Path, prompt: &str, rows: &[Value]) {
    let state = Store::read(worker).unwrap().unwrap();
    delivery_outcome::begin(&state, REQUEST, prompt, fs::metadata(path).unwrap().len()).unwrap();
    use std::io::Write;
    let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
    for row in rows {
        writeln!(file, "{row}").unwrap();
    }
}

/// A Codex observer declined the checkpoint after tool outputs were cut.
/// Other turns read the same evidence, so one fresh turn of the same request
/// is worth it; repeated polls of one decline never add more.
#[test]
fn a_refusal_is_redelivered_once_and_repeated_polls_never_add_a_second() {
    let (worker, root, path, _) = refusal_fixture("codex");
    for _ in 0..13 {
        delivery_outcome::observe(&worker).unwrap();
    }
    assert!(!root.join("delivery-outcome/terminal.json").exists());
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        REQUEST
    );
    let gap: Value = source_store::read(&root.join("gap.json")).unwrap();
    assert!(
        gap["error"]
            .as_str()
            .unwrap()
            .starts_with("REVIEW_OBSERVER_REFUSAL_RETRY stage=delivery")
    );
    assert_eq!(gap["request"], REQUEST);
    assert!(redelivery::gap_retryable(&gap));
    assert_eq!(
        fs::read_dir(root.join("delivery-outcome/refused"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(spawns(&root, "__review-retry"), 1);
    fs::remove_file(path).unwrap();
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

/// A second decline of the same request retires that request only, as NOT
/// reviewed with a gap and a notice. Later triggers are admitted again: a
/// steering job must not wait behind the retained refusal.
#[test]
fn a_second_refusal_retires_only_that_request_and_the_queue_continues() {
    for provider in ["codex", "claude"] {
        let (worker, root, path, prompt) = refusal_fixture(provider);
        // An intent-sync steering job queued behind the refusal.
        queue::enqueue(
            &worker,
            Purpose::Steering,
            "intent-sync",
            json!({"later":true}),
        )
        .unwrap();
        delivery_outcome::observe(&worker).unwrap();
        assert!(
            queue::claim_batch(&worker).unwrap().is_none(),
            "retry holds the queue"
        );
        redeliver(
            &worker,
            &path,
            &prompt,
            &records(provider, &prompt, &refusal()),
        );
        for _ in 0..3 {
            delivery_outcome::observe(&worker).unwrap();
        }
        // The retry job, not the observation, retires it.
        assert!(delivery_outcome::retire_terminal(&worker).unwrap());
        assert!(Store::read(&worker).unwrap().unwrap().pending.is_none());
        assert!(!delivery_outcome::retire_terminal(&worker).unwrap());
        let result: Value =
            source_store::read(&root.join(format!("results/{REQUEST}.json"))).unwrap();
        assert_eq!(result["status"], "not_reviewed");
        assert_eq!(result["gap"]["code"], "REVIEW_OBSERVER_REFUSED");
        let gap: Value = source_store::read(&root.join("gap.json")).unwrap();
        assert!(
            gap["error"]
                .as_str()
                .unwrap()
                .starts_with("REVIEW_OBSERVER_REFUSED stage=observer")
        );
        assert!(!redelivery::gap_retryable(&gap));
        let outcome: Value =
            source_store::read(&root.join("delivery-outcome/terminal.json")).unwrap();
        assert_eq!(outcome["unanswered"], 2);
        assert!(
            root.join("delivery-outcome/provider-block-notice.json")
                .is_file()
        );
        assert!(spawns(&root, "__review-wake") >= 1, "the queue is kicked");
        let (later, _) = queue::claim_batch(&worker)
            .unwrap()
            .expect("the queue continues");
        assert_eq!(later.trigger["later"], true);
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}

#[test]
fn free_form_backstop_survives_restart_but_a_submitted_report_is_not_unanswered() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated test state");
    for reported in [false, true] {
        let worker = format!("delivery-outcome-{}", kah_cli::relay_store::nonce());
        let store = Store::open(&worker).unwrap();
        let root = store.directory.clone();
        let prompt = prompt();
        let first = records("claude", &prompt, "I decline this request.");
        let path = native(&first);
        let state = state(&root, &worker, &path);
        store.save(&state).unwrap();
        drop(store);
        delivery_outcome::begin(&state, REQUEST, &prompt, 0).unwrap();
        delivery_outcome::observe(&worker).unwrap();
        // Repeat polls never turn one native response into two attempts.
        delivery_outcome::observe(&worker).unwrap();
        delivery_outcome::begin(&state, REQUEST, &prompt, fs::metadata(&path).unwrap().len())
            .unwrap();
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        for row in &first {
            writeln!(file, "{row}").unwrap();
        }
        drop(file);
        if reported {
            source_store::save(
                &root.join("outbox"),
                &format!("submitted-{REQUEST}.json"),
                &json!({"evidence_submitted":true}),
            )
            .unwrap();
            delivery_outcome::observe(&worker).unwrap();
            assert!(!root.join("delivery-outcome/terminal.json").exists());
        } else {
            for _ in 0..12 {
                assert!(
                    delivery_outcome::observe(&worker)
                        .unwrap_err()
                        .to_string()
                        .starts_with("DELIVERY_UNANSWERED stage=delivery")
                );
            }
            let outcome: Value =
                source_store::read(&root.join("delivery-outcome/terminal.json")).unwrap();
            assert_eq!(outcome["unanswered"], 2);
        }
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}

#[test]
fn incomplete_work_other_session_and_changed_body_are_not_terminal_refusals() {
    let prompt = format!("kah review request {REQUEST} (Checkpoint). Read evidence.");
    let mut rows = records("claude", &prompt, "working on evidence");
    rows[1]["message"]["stop_reason"] = Value::Null;
    let mut other = rows[1].clone();
    other["sessionId"] = json!("other-native-session");
    other["message"]["stop_reason"] = json!("end_turn");
    rows.push(other);
    let mut sidechain = rows[1].clone();
    sidechain["isSidechain"] = json!(true);
    sidechain["message"]["stop_reason"] = json!("end_turn");
    rows.push(sidechain);
    let path = native(&rows);
    let mut progress = delivery_outcome::Progress::default();
    assert!(
        delivery_outcome::scan(
            "claude",
            SESSION,
            &path,
            REQUEST,
            &digest(&prompt),
            &mut progress
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(progress.unanswered, 0);
    let mut unmatched = delivery_outcome::Progress::default();
    assert!(
        delivery_outcome::scan(
            "claude",
            SESSION,
            &path,
            REQUEST,
            &digest("different body"),
            &mut unmatched
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(unmatched.unanswered, 0);
    fs::remove_file(path).unwrap();
}

#[test]
fn codex_receives_instruction_and_request_in_one_paste_so_no_typed_text_is_dropped() {
    // Codex's paste-burst buffer once dropped the typed
    // instruction's tail when the request paste arrived, so the native row
    // contain an incomplete instruction and no matching receipt.
    let prompt = prompt();
    let mut sent = vec![];
    delivery_text::submit("codex", "private-pane", &prompt, |args| {
        sent.push(args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        sent,
        vec![vec![
            "agent".to_string(),
            "prompt".into(),
            "private-pane".into(),
            delivery_text::cold(&prompt)
        ]]
    );
    let marker = format!("kah review request {REQUEST} ");
    let row = |text: String| {
        format!(
            "{}\n",
            json!({"type":"response_item","payload":{"type":"message","role":"user",
                "content":[{"type":"input_text","text":text}]}})
        )
    };
    let whole = row(delivery_text::cold(&prompt));
    assert!(delivery::receipt(Cursor::new(&whole), "codex", SESSION, &marker).unwrap());
    // A damaged instruction is still not accepted as proof of this request.
    let cut = delivery_text::INSTRUCTION.find("reason>").unwrap();
    let damaged = row(format!("{}{prompt}", &delivery_text::INSTRUCTION[..cut]));
    assert!(!delivery::receipt(Cursor::new(&damaged), "codex", SESSION, &marker).unwrap());
}

/// When the host's container daemon restarts, the Codex observer's sandbox
/// exec-server stops and every command is rejected at process creation, so
/// the observer declines the request. That is a lost observer process, not a
/// review decision.
fn exec_lost_rows(prompt: &str, reply: &str) -> Vec<Value> {
    let mut rows = records("codex", prompt, reply);
    rows.insert(
        1,
        json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution",
            "status":"failed","exit_code":-1,"source":"unified_exec_startup",
            "aggregated_output":"Failed to create unified exec process: exec-server transport disconnected"}}}),
    );
    rows
}
fn refusal() -> String {
    json!({"outcome":"DELIVERY_REFUSED","request":REQUEST,"reason":"The exec-server transport is disconnected"})
        .to_string()
}

#[test]
fn a_turn_whose_commands_could_not_start_is_an_exec_loss_not_only_a_refusal() {
    let prompt = format!("kah review request {REQUEST} (Checkpoint). Read evidence.");
    for (rows, lost) in [
        (exec_lost_rows(&prompt, &refusal()), true),
        (exec_lost_rows(&prompt, "No report."), true),
        (records("codex", &prompt, &refusal()), false),
    ] {
        let path = native(&rows);
        let mut progress = delivery_outcome::Progress::default();
        let outcome = delivery_outcome::scan(
            "codex",
            SESSION,
            &path,
            REQUEST,
            &digest(&prompt),
            &mut progress,
        )
        .unwrap()
        .expect("a refusal or a lost exec transport ends the delivery");
        assert_eq!(outcome["exec_lost"] == true, lost);
        fs::remove_file(path).unwrap();
    }
}

/// After the one redelivery, any end of the request retires it with its
/// reason; a second exec loss that ends unanswered must not jam the queue.
#[test]
fn any_end_after_the_one_redelivery_retires_the_request_with_its_reason() {
    for exec_lost in [true, false] {
        let (worker, root, path, prompt) = refusal_fixture("codex");
        let rows = |reply: &str| {
            if exec_lost {
                exec_lost_rows(&prompt, reply)
            } else {
                records("codex", &prompt, reply)
            }
        };
        let first: String = rows(&refusal()).iter().map(|r| format!("{r}\n")).collect();
        fs::write(&path, first).unwrap();
        delivery_outcome::observe(&worker).unwrap();
        redeliver(&worker, &path, &prompt, &rows("No report."));
        delivery_outcome::observe(&worker).unwrap();
        assert!(delivery_outcome::retire_terminal(&worker).unwrap());
        assert!(Store::read(&worker).unwrap().unwrap().pending.is_none());
        let result: Value =
            source_store::read(&root.join(format!("results/{REQUEST}.json"))).unwrap();
        assert_eq!(
            result["gap"]["code"], "REVIEW_OBSERVER_NO_REPORT",
            "{exec_lost}"
        );
        fs::remove_file(path).unwrap();
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}

#[test]
fn an_exec_loss_replaces_the_observer_once_then_stays_terminal() {
    let (worker, root, path, prompt) = refusal_fixture("codex");
    fs::write(
        &path,
        exec_lost_rows(&prompt, &refusal())
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    delivery_outcome::observe(&worker).unwrap();
    assert!(!root.join("delivery-outcome/terminal.json").exists());
    let gap: Value = source_store::read(&root.join("gap.json")).unwrap();
    assert!(
        gap["error"]
            .as_str()
            .unwrap()
            .starts_with("REVIEW_OBSERVER_EXEC_LOST stage=delivery")
    );
    assert!(redelivery::gap_retryable(&gap));
    assert!(delivery_outcome::replacement_requested(&root, REQUEST).unwrap());
    assert!(!delivery_outcome::replacement_requested(&root, "review-other").unwrap());
    // The fresh observer lost its exec transport too: no second replacement
    // and no refusal retry; this second refusal retires the request.
    redeliver(
        &worker,
        &path,
        &prompt,
        &exec_lost_rows(&prompt, &refusal()),
    );
    delivery_outcome::observe(&worker).unwrap();
    assert!(delivery_outcome::retire_terminal(&worker).unwrap());
    assert!(Store::read(&worker).unwrap().unwrap().pending.is_none());
    assert!(!root.join("delivery-outcome/refused").exists());
    let result: Value = source_store::read(&root.join(format!("results/{REQUEST}.json"))).unwrap();
    assert_eq!(result["gap"]["code"], "REVIEW_OBSERVER_REFUSED");
    fs::remove_file(path).unwrap();
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
