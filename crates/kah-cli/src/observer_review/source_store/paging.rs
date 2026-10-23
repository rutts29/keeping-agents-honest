//! Pages of `kah observe read-source` output. Codex truncates long tool output (about 10,000 tokens at the
//! time of writing), so a large source is served in pages well under that, split on
//! line boundaries and never inside a UTF-8 char.
use crate::Result;
use std::ops::Range;

/// UTF-8 bytes per page. Hex digests cost about 2 bytes per token, so a page stays near 8,000 tokens.
pub const PAGE_BYTES: usize = 16_000;
/// A line boundary is used only if it leaves a page at least this full; otherwise a long line is cut.
const MIN_FILL: usize = PAGE_BYTES / 4;

/// Byte ranges of the pages, in order. They cover the text exactly and never split a character.
pub fn bounds(text: &str) -> Vec<Range<usize>> {
    let mut pages = vec![];
    let mut start = 0;
    while text.len() - start > PAGE_BYTES {
        let mut limit = start + PAGE_BYTES;
        while !text.is_char_boundary(limit) {
            limit -= 1;
        }
        let end = match text[start..limit].rfind('\n') {
            Some(at) if at + 1 >= MIN_FILL => start + at + 1,
            _ => limit,
        };
        pages.push(start..end);
        start = end;
    }
    pages.push(start..text.len());
    pages
}
pub fn count(text: &str) -> usize {
    bounds(text).len()
}
/// Page `page` (1-based). A source of one page is returned as it is; every other page but the last ends with
/// `\n[page N of M - continue: COMMAND]\n`, where `command(N + 1)` is the exact command for the next page.
pub fn render(text: &str, page: usize, command: &dyn Fn(usize) -> String) -> Result<String> {
    let pages = bounds(text);
    let total = pages.len();
    if page == 0 || page > total {
        return Err(format!(
            "REVIEW_SOURCE_PAGE stage=read_source: page {page} does not exist; this source has {total} page{}",
            if total == 1 { "" } else { "s" }
        )
        .into());
    }
    let mut out = text[pages[page - 1].clone()].to_owned();
    if page < total {
        out.push_str(&format!(
            "\n[page {page} of {total} - continue: {}]\n",
            command(page + 1)
        ));
    }
    Ok(out)
}
/// The command that serves `page` of the source the reader named, quoted for a shell.
pub fn command(path: &str, page: usize) -> String {
    let safe = !path.is_empty()
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./:@%+=,-".contains(&b));
    let path = if safe {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    };
    format!("bin/kah observe read-source {path} --page {page}")
}
/// `read-source PATH [--page N]`: the path and the 1-based page (1 without the flag).
pub fn arguments(args: &[String]) -> Option<(&str, usize)> {
    match args {
        [_, _, path] => Some((path, 1)),
        [_, _, path, flag, page] if flag == "--page" => {
            page.parse().ok().map(|page| (path.as_str(), page))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/source_paging.rs"]
mod tests;
