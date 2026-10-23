//! #52b through the binary the observer runs: `kah observe read-source PATH [--page N]` pages a 67k-token source,
//! the footer command works as printed, and `kah observe report` refuses a submit until every page was read.
use kah_cli::observer_review::source_store;
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn kah(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_kah"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}
fn out(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}
/// 264,592 bytes: the size of the 878-12c option1.txt (66,754 tokens in Codex's count).
fn source() -> String {
    let mut text = String::new();
    let mut n = 0u64;
    while text.len() < 264_592 {
        text.push_str(&format!("{n:06} \"digest\": \"{:064x}\" é漢😀\n", n * 7919));
        n += 1;
    }
    text
}
/// `<ws>/inputs/review-1` with `record.txt` stored as a source descriptor and listed in context-input.json.
fn workspace(text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("source-paged-{}", kah_cli::relay_store::nonce()));
    let (private, ws) = (root.join("private"), root.join("observer"));
    let view = ws.join("inputs/review-1");
    fs::create_dir_all(&view).unwrap();
    fs::create_dir_all(ws.join("outbox")).unwrap();
    source_store::initialize(&private, &ws).unwrap();
    source_store::text(&private, &view.join("record.txt"), text).unwrap();
    source_store::save(&view, "request.json", &json!({"head":"unit"})).unwrap();
    fs::write(
        view.join("context-input.json"),
        json!({"sources":[{"id":"record","path":view.join("record.txt")}]}).to_string(),
    )
    .unwrap();
    source_store::share(&private, &view).unwrap();
    (root, ws)
}
/// Runs the command a footer printed, exactly as printed.
fn run_footer(ws: &Path, page: &str) -> std::process::Output {
    let footer = page.trim_end().lines().last().unwrap();
    let command = footer
        .split(" - continue: ")
        .nth(1)
        .unwrap()
        .trim_end_matches(']');
    let words: Vec<&str> = command.split(' ').collect();
    assert_eq!(&words[..3], ["bin/kah", "observe", "read-source"]);
    kah(ws, &words[1..])
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn a_67k_token_source_pages_through_its_footers_and_concatenates_to_the_original() {
    let text = source();
    let (root, ws) = workspace(&text);
    let path = ws.join("inputs/review-1/record.txt");
    let first = kah(&ws, &["observe", "read-source", path.to_str().unwrap()]);
    assert!(first.status.success(), "{}", err(&first));
    let mut page = out(&first);
    assert!(
        page.len() < 16_300 && page.contains(" of 17 - continue: bin/kah observe read-source ")
    );
    let mut all = String::new();
    loop {
        match page.rfind("\n[page ").filter(|_| page.ends_with("]\n")) {
            Some(at) => {
                all.push_str(&page[..at]);
                let next = run_footer(&ws, &page);
                assert!(next.status.success(), "{}", err(&next));
                page = out(&next);
            }
            None => {
                all.push_str(&page);
                break;
            }
        }
    }
    assert_eq!(all, text);
    let past = kah(
        &ws,
        &[
            "observe",
            "read-source",
            path.to_str().unwrap(),
            "--page",
            "18",
        ],
    );
    assert!(
        err(&past).contains("REVIEW_SOURCE_PAGE stage=read_source"),
        "{}",
        err(&past)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn a_source_that_fits_one_page_is_served_unchanged() {
    let text = "small source\n".repeat(100);
    let (root, ws) = workspace(&text);
    let path = ws.join("inputs/review-1/record.txt");
    let page = kah(&ws, &["observe", "read-source", path.to_str().unwrap()]);
    assert_eq!(page.stdout, text.as_bytes());
    fs::remove_dir_all(root).unwrap();
}

/// Serves pages `first..=last` of the source the way the observer reads it.
fn read_pages(ws: &Path, record: &Path, first: usize, last: usize) {
    for page in first..=last {
        let page = page.to_string();
        let args = [
            "observe",
            "read-source",
            record.to_str().unwrap(),
            "--page",
            &page,
        ];
        let read = kah(ws, &args);
        assert!(read.status.success(), "{}", err(&read));
    }
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn report_refuses_a_submit_until_every_page_of_the_listed_evidence_was_read() {
    let (root, ws) = workspace(&source());
    let view = ws.join("inputs/review-1");
    let (record, request) = (view.join("record.txt"), view.join("request.json"));
    let report = ws.join("outbox/report.json");
    fs::write(&report, "{}").unwrap();
    let submit = || {
        let args = [
            "observe",
            "report",
            "--request",
            request.to_str().unwrap(),
            "--json",
            report.to_str().unwrap(),
        ];
        kah(&ws, &args)
    };
    read_pages(&ws, &record, 1, 2);
    let refused = submit();
    let message = err(&refused);
    assert!(!refused.status.success());
    assert!(
        message.contains("REVIEW_EVIDENCE_UNREAD stage=report"),
        "{message}"
    );
    assert!(
        message.contains("record.txt: pages 3-17 of 17"),
        "{message}"
    );
    let page3 = format!("bin/kah observe read-source {} --page 3", record.display());
    assert!(message.contains(&page3), "{message}");
    read_pages(&ws, &record, 3, 17);
    // Everything was read: the submit now fails (or passes) on its own merits, never on unread evidence.
    assert!(!err(&submit()).contains("REVIEW_EVIDENCE_UNREAD"));
    fs::remove_dir_all(root).unwrap();
}
