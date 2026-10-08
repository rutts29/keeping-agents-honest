//! Immutable validation scope reuses checks; new scopes still reject tampering.
use kah_observer::review_evidence::{Citation, Document, VerifiedSources};
#[test]
fn repeated_whole_quotes_hash_a_source_once_without_changing_evidence() {
    let doc = Document::new(
        "source",
        "transcript",
        "native:one",
        "First sentence.\nSecond sentence.\n",
    );
    let docs = vec![doc.clone()];
    let verifier = VerifiedSources::new(&docs);
    let a = Citation {
        source: doc.id.clone(),
        digest: doc.digest.clone(),
        quote: "First sentence.".into(),
        occurrence: 0,
    };
    let b = Citation {
        quote: "Second sentence.".into(),
        ..a.clone()
    };
    let first = verifier.quote(&a).unwrap();
    assert_eq!(verifier.quote(&a).unwrap(), first);
    let second = verifier.quote(&b).unwrap();
    assert_eq!(second["line"], 2);
    assert_eq!(verifier.digest_checks(), 1);
    assert_eq!(verifier.quote_checks(), 2);
    assert_eq!(first["quote"], a.quote);
    assert_eq!(second["quote"], b.quote);
}
#[test]
fn a_new_scope_never_reuses_old_content_or_accepts_a_forged_digest() {
    let mut doc = Document::new("source", "transcript", "native:one", "Whole sentence.");
    let citation = Citation {
        source: doc.id.clone(),
        digest: doc.digest.clone(),
        quote: doc.text.clone(),
        occurrence: 0,
    };
    assert!(
        VerifiedSources::new(&[doc.clone()])
            .quote(&citation)
            .is_ok()
    );
    doc.text.push_str(" Forged content.");
    assert!(
        VerifiedSources::new(&[doc])
            .quote(&citation)
            .unwrap_err()
            .contains("REVIEW_SOURCE_CHANGED")
    );
}
