//! VF16: a native decline must never become thirteen automatic pasted wakes.
use kah_cli::observer_review::{
    delivery, delivery_outcome, delivery_text,
    failure::Origin,
    redelivery, request_context, source_store,
    state::{State, Store},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::PathBuf};

const REQUEST: &str = "review-1000000000013-1";
const SESSION: &str = "00000000-0000-4000-8000-100000000012";

#[test]
fn terminal_delivery_outcomes_are_never_report_corrections() {
    for code in ["DELIVERY_REFUSED", "DELIVERY_UNANSWERED"] {
        assert!(!redelivery::retryable(
            Origin::Delivery,
            &format!("{code} stage=delivery: review-1000000000013-1")
        ));
    }
}

fn retained() -> Vec<Value> {
    include_str!("fixtures/delivery-outcome/claude-refusal.jsonl")
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
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
fn retained_free_form_refusal_stops_after_two_deliveries_not_thirteen() {
    let rows = retained();
    let body_digest = digest(&body(&rows[0]));
    // The correction removes only the host's invented report-error diagnostic
    // on missing replies. Replay all 13 EXACT retained assistant answers against
    // the unchanged original user body, not a newly classified prose signature.
    let replay: Vec<Value> = rows
        .chunks_exact(2)
        .flat_map(|pair| [rows[0].clone(), pair[1].clone()])
        .collect();
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
    let prompt = body(&retained()[0]);
    let mut sent = vec![];
    delivery_text::submit("private-pane", &prompt, |args| {
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

#[test]
fn refusal_survives_restart_blocks_twelve_retries_and_keeps_pending_and_notice() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated test state");
    let worker = format!("delivery-outcome-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    let prompt = body(&retained()[0]);
    let reply = json!({"outcome":"DELIVERY_REFUSED","request":REQUEST,"reason":"native decline"})
        .to_string();
    let path = native(&records("claude", &prompt, &reply));
    let state = state(&root, &worker, &path);
    store.save(&state).unwrap();
    drop(store);
    delivery_outcome::begin(&state, REQUEST, &prompt, 0).unwrap();
    for _ in 0..13 {
        assert!(
            delivery_outcome::observe(&worker)
                .unwrap_err()
                .to_string()
                .starts_with("DELIVERY_REFUSED stage=delivery")
        );
    }
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        REQUEST
    );
    let outcome: Value = source_store::read(&root.join("delivery-outcome/terminal.json")).unwrap();
    assert_eq!(outcome["body_digest"], digest(&prompt));
    assert_eq!(outcome["unanswered"], 1);
    let notice: Value =
        source_store::read(&root.join("delivery-outcome/provider-block-notice.json")).unwrap();
    assert_eq!(notice["submitted"], false, "no live socket in this test");
    fs::remove_file(path).unwrap();
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn free_form_backstop_survives_restart_but_a_submitted_report_is_not_unanswered() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated test state");
    for reported in [false, true] {
        let worker = format!("delivery-outcome-{}", kah_cli::relay_store::nonce());
        let store = Store::open(&worker).unwrap();
        let root = store.directory.clone();
        let prompt = body(&retained()[0]);
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
