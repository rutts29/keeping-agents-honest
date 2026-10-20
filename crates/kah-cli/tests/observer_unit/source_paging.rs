//! #52b: read-source pages. Each page stays under the byte budget, pages concatenate to the exact source,
//! and every page but the last ends with the footer line naming the exact command for the next page.
use super::*;

fn command(page: usize) -> String {
    format!("bin/kah observe read-source inputs/r/transcript.txt --page {page}")
}
/// Everything the reader would see by following the footers from page 1.
fn follow(text: &str) -> (Vec<String>, String) {
    let mut pages = vec![];
    let mut page = 1;
    loop {
        let out = render(text, page, &command).unwrap();
        pages.push(out.clone());
        match out.rfind("\n[page ") {
            Some(at) => {
                let footer = out[at + 1..].trim_end_matches('\n');
                let want = format!(
                    "[page {page} of {} - continue: {}]",
                    count(text),
                    command(page + 1)
                );
                assert_eq!(footer, want);
                assert!(out.ends_with(&format!("{want}\n")));
                page += 1;
            }
            None => break,
        }
    }
    let joined = pages
        .iter()
        .map(|p| p.rfind("\n[page ").map_or(p.as_str(), |at| &p[..at]))
        .collect::<String>();
    (pages, joined)
}
/// About the size of the 67k-token option1.txt of 878-12c: 264,592 bytes, mixed ASCII and multibyte.
fn big(lines: bool) -> String {
    let mut text = String::new();
    let mut n = 0;
    while text.len() < 264_592 {
        text.push_str(&format!(
            "{n:06} \"digest\": \"{:064x}\" é漢😀 end",
            n * 7919
        ));
        if lines {
            text.push('\n');
        }
        n += 1;
    }
    text
}

#[test]
fn a_large_multiline_source_pages_and_concatenates_to_the_original() {
    let text = big(true);
    let (pages, joined) = follow(&text);
    assert_eq!(joined, text);
    assert_eq!(pages.len(), count(&text));
    assert!(pages.len() > 10);
    for page in &pages {
        assert!(page.len() <= PAGE_BYTES + 200, "{} bytes", page.len());
    }
}

#[test]
fn a_single_huge_line_splits_without_cutting_a_character() {
    let text = big(false);
    assert!(!text.contains('\n'));
    let (pages, joined) = follow(&text);
    assert_eq!(joined, text);
    for page in &pages {
        assert!(page.len() <= PAGE_BYTES + 200);
    }
}

#[test]
fn pages_break_after_a_newline_when_one_is_in_reach() {
    let text = big(true);
    let (pages, _) = follow(&text);
    for page in &pages[..pages.len() - 1] {
        let body = &page[..page.rfind("\n[page ").unwrap()];
        assert!(
            body.ends_with('\n'),
            "page ends mid-line: {:?}",
            &body[body.len().saturating_sub(20)..]
        );
    }
}

#[test]
fn a_source_that_fits_one_page_is_unchanged() {
    for text in ["", "short\n", "no newline", &"é".repeat(PAGE_BYTES / 2)] {
        assert_eq!(count(text), 1);
        assert_eq!(render(text, 1, &command).unwrap(), text);
    }
}

#[test]
fn a_page_outside_the_source_is_a_stable_error() {
    let text = big(true);
    let last = count(&text);
    for page in [0, last + 1] {
        let error = render(&text, page, &command).unwrap_err().to_string();
        assert!(
            error.starts_with("REVIEW_SOURCE_PAGE stage=read_source"),
            "{error}"
        );
    }
    assert!(render("small", 2, &command).is_err());
}

#[test]
fn the_continue_command_quotes_the_path_and_the_flag_parses_back() {
    assert_eq!(
        super::command("inputs/r/transcript.txt", 3),
        "bin/kah observe read-source inputs/r/transcript.txt --page 3"
    );
    assert_eq!(
        super::command("in puts/it's", 2),
        "bin/kah observe read-source 'in puts/it'\\''s' --page 2"
    );
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        arguments(&args(&["observe", "read-source", "p"])),
        Some(("p", 1))
    );
    assert_eq!(
        arguments(&args(&["observe", "read-source", "p", "--page", "4"])),
        Some(("p", 4))
    );
    for bad in [
        &["observe", "read-source", "p", "--page"][..],
        &["observe", "read-source", "p", "--page", "x"],
        &["observe", "read-source", "p", "--offset", "4"],
        &["observe", "read-source"],
    ] {
        assert_eq!(arguments(&args(bad)), None);
    }
}
