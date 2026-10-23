//! The read ledger and the submit check. A source of 7 pages read up to page 2 blocks the submit with one
//! plain message naming the unread pages and their exact commands; once every page was served it passes.
use super::*;
use crate::observer_review::source_store::paging;
use std::fs;

struct View {
    root: std::path::PathBuf,
    request: std::path::PathBuf,
    sources: Vec<(std::path::PathBuf, String)>,
}
impl Drop for View {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn seven_pages() -> String {
    let mut text = String::new();
    let mut n = 0;
    while paging::count(&text) < 7 {
        text.push_str(&format!("{n:06} line of evidence é漢 {:032x}\n", n * 31));
        n += 1;
    }
    text
}
/// `<ws>/inputs/review-1/{request.json,context-input.json,<sources>}`, plain-text sources.
fn view(texts: &[(&str, String)]) -> View {
    let root =
        std::env::temp_dir().join(format!("kah-read-ledger-{}", crate::relay_store::nonce()));
    let dir = root.join("inputs/review-1");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(root.join("outbox")).unwrap();
    let mut sources = vec![];
    for (name, text) in texts {
        fs::write(dir.join(name), text).unwrap();
        sources.push((dir.join(name), text.clone()));
    }
    let listed: Vec<_> = sources
        .iter()
        .map(|(p, _)| serde_json::json!({"path": p}))
        .collect();
    fs::write(
        dir.join("context-input.json"),
        serde_json::json!({"sources": listed}).to_string(),
    )
    .unwrap();
    fs::write(dir.join("request.json"), "{}").unwrap();
    View {
        request: dir.join("request.json"),
        root,
        sources,
    }
}
fn serve(view: &View, source: usize, pages: std::ops::RangeInclusive<usize>) {
    let (path, text) = &view.sources[source];
    for page in pages {
        record(path, text, page, paging::count(text)).unwrap();
    }
}

#[test]
fn a_submit_after_pages_1_and_2_of_7_is_rejected_with_the_unread_pages_and_commands() {
    let text = seven_pages();
    assert_eq!(paging::count(&text), 7);
    let v = view(&[("transcript.txt", text)]);
    serve(&v, 0, 1..=2);
    let error = require_read(&v.request).unwrap_err().to_string();
    let path = v.sources[0].0.display();
    assert!(
        error.starts_with("REVIEW_EVIDENCE_UNREAD stage=report"),
        "{error}"
    );
    assert!(error.contains("transcript.txt: pages 3-7 of 7"), "{error}");
    for page in 3..=7 {
        assert!(
            error.contains(&format!("bin/kah observe read-source {path} --page {page}")),
            "{error}"
        );
    }
    assert!(
        !error.contains("--page 2") && !error.contains("--page 1"),
        "{error}"
    );
}

#[test]
fn a_submit_after_every_page_was_served_is_accepted() {
    let v = view(&[("transcript.txt", seven_pages())]);
    serve(&v, 0, 1..=7);
    require_read(&v.request).unwrap();
}

#[test]
fn pages_read_out_of_order_or_twice_count_once_and_a_gap_is_named() {
    let v = view(&[("transcript.txt", seven_pages())]);
    for page in [7, 1, 1, 2, 3, 5, 6] {
        serve(&v, 0, page..=page);
    }
    let error = require_read(&v.request).unwrap_err().to_string();
    assert!(error.contains("page 4 of 7"), "{error}");
}

#[test]
fn a_small_source_counts_as_read_when_read_once_and_each_source_is_checked() {
    let v = view(&[
        ("option1.txt", "small\n".into()),
        ("transcript.txt", seven_pages()),
    ]);
    assert!(
        require_read(&v.request)
            .unwrap_err()
            .to_string()
            .contains("option1.txt: page 1 of 1")
    );
    serve(&v, 0, 1..=1);
    serve(&v, 1, 1..=6);
    let error = require_read(&v.request).unwrap_err().to_string();
    assert!(
        error.contains("transcript.txt: page 7 of 7") && !error.contains("option1.txt"),
        "{error}"
    );
    serve(&v, 1, 7..=7);
    require_read(&v.request).unwrap();
}

#[test]
fn reads_are_kept_per_request_and_per_content() {
    let v = view(&[("transcript.txt", seven_pages())]);
    serve(&v, 0, 1..=7);
    // The same file in another request's view has its own ledger, and changed content is a different source.
    let other = v.root.join("inputs/review-2");
    fs::create_dir_all(&other).unwrap();
    fs::copy(
        v.root.join("inputs/review-1/context-input.json"),
        other.join("context-input.json"),
    )
    .unwrap();
    fs::write(other.join("request.json"), "{}").unwrap();
    assert!(require_read(&other.join("request.json")).is_err());
    fs::write(&v.sources[0].0, format!("{}more\n", v.sources[0].1)).unwrap();
    assert!(require_read(&v.request).is_err());
}

#[test]
fn a_request_without_a_source_list_has_nothing_required() {
    let v = view(&[]);
    require_read(&v.request).unwrap();
    fs::remove_file(v.request.with_file_name("context-input.json")).unwrap();
    require_read(&v.request).unwrap();
}
