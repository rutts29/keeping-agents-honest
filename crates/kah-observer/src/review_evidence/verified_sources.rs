//! A borrowed, immutable document set; no cache crosses validation calls.
use super::*;
use std::{cell::RefCell, collections::HashMap};
pub struct VerifiedSources<'a> {
    documents: &'a [Document],
    digests: RefCell<HashMap<String, bool>>,
    quotes: RefCell<HashMap<(String, String, String, usize), Value>>,
}
impl<'a> VerifiedSources<'a> {
    pub fn new(documents: &'a [Document]) -> Self {
        Self {
            documents,
            digests: RefCell::new(HashMap::new()),
            quotes: RefCell::new(HashMap::new()),
        }
    }
    pub fn digest_checks(&self) -> usize {
        self.digests.borrow().len()
    }
    pub fn quote_checks(&self) -> usize {
        self.quotes.borrow().len()
    }
    pub fn quote(&self, citation: &Citation) -> Result<Value, String> {
        let key = (
            citation.source.clone(),
            citation.digest.clone(),
            citation.quote.clone(),
            citation.occurrence,
        );
        if let Some(saved) = self.quotes.borrow().get(&key).cloned() {
            return Ok(saved);
        }
        let document=self.documents.iter().find(|d|d.id==citation.source).ok_or_else(|| {
            let aliases=self.documents.iter().filter(|d|d.uri==citation.source).map(|d|d.id.as_str()).collect::<Vec<_>>();
            format!("REVIEW_SOURCE_UNKNOWN stage=evidence: citation.source={} is not a document id. Use manifest document ids, not URIs or paths. Matching URI document ids: {}",json!(citation.source),json!(aliases))
        })?;
        if citation.digest != document.digest {
            return Err("REVIEW_SOURCE_CHANGED stage=evidence".into());
        }
        let known = self.digests.borrow().get(&document.id).copied();
        let matches = known.unwrap_or_else(|| {
            let matches =
                format!("sha256:{:x}", Sha256::digest(document.text.as_bytes())) == document.digest;
            self.digests
                .borrow_mut()
                .insert(document.id.clone(), matches);
            matches
        });
        if !matches {
            return Err("REVIEW_SOURCE_CHANGED stage=evidence".into());
        }
        let evidence = verified_quote(citation, document)?;
        self.quotes.borrow_mut().insert(key, evidence.clone());
        Ok(evidence)
    }
}
fn verified_quote(citation: &Citation, document: &Document) -> Result<Value, String> {
    let digest = &document.digest;
    if citation.quote.is_empty() {
        return Err("REVIEW_QUOTE_EMPTY stage=evidence".into());
    }
    let (start, _) = document
        .text
        .match_indices(&citation.quote)
        .nth(citation.occurrence)
        .ok_or("REVIEW_QUOTE_MISSING stage=evidence")?;
    let end = start + citation.quote.len();
    if joins_word(
        document.text[..start].chars().next_back(),
        citation.quote.chars().next(),
    ) || joins_word(
        citation.quote.chars().next_back(),
        document.text[end..].chars().next(),
    ) {
        return Err("REVIEW_QUOTE_UNIT stage=evidence: the brain must select a complete meaningful unit, never part of a word".into());
    }
    let line = document.text[..start]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    // The brain chooses a complete meaningful span, including any nearby
    // context it needs. Keep the full source in the immutable snapshot/CAS;
    // repeating it here would undo evidence extraction and swamp Jev.
    let evidence = json!({"source":document.id,"kind":document.kind,"uri":document.uri,"digest":digest,"quote":citation.quote,"start_byte":start,"end_byte":end,"line":line});
    Ok(with_native_records(evidence, document, start, end))
}
