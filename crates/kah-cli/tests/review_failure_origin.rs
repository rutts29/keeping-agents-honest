use kah_cli::observer_review::failure::{Origin, correctable};
use std::{collections::BTreeSet, fs, path::Path};
#[test]
fn every_review_code_is_classified_by_origin_with_only_explicit_identity_integrity_exceptions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut codes = BTreeSet::new();
    collect(root, &mut codes);
    assert!(codes.contains("REVIEW_SOURCE_CHANGED"));
    assert!(codes.len() > 200);
    for code in &codes {
        assert!(!correctable(Origin::Host, code), "host: {code}");
        for origin in [Origin::ReportValidation, Origin::SelectionValidation] {
            let fatal = code.starts_with("REVIEW_SESSION_")
                || code.starts_with("REVIEW_HOOK_SCOPE")
                || code == "REVIEW_REQUEST_MISMATCH"
                || code == "REVIEW_REQUEST_ID"
                || code == "REVIEW_SOURCE_DIGEST"
                || code == "REVIEW_COMPLETION_RECEIPT_BINDING"
                || code == "REVIEW_CONTROLLER_FACT_BINDING"
                || code == "REVIEW_QUESTION_REFERENCE_SCOPE";
            assert_eq!(correctable(origin, code), !fatal, "{origin:?}: {code}");
        }
    }
    assert!(correctable(
        Origin::ReportValidation,
        "REVIEW_FUTURE_UNLISTED_SCHEMA_ERROR"
    ));
    assert!(!correctable(
        Origin::Host,
        "REVIEW_FUTURE_UNLISTED_SCHEMA_ERROR"
    ));
}
fn collect(path: &Path, codes: &mut BTreeSet<String>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, codes);
        } else if path.extension().is_some_and(|s| s == "rs")
            && path.components().any(|c| c.as_os_str() == "src")
        {
            for word in fs::read_to_string(path)
                .unwrap()
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            {
                if word.starts_with("REVIEW_") {
                    codes.insert(word.to_owned());
                }
            }
        }
    }
}
