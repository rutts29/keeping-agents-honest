use super::*;

#[test]
fn source_cursor_compares_the_exact_accepted_prefix() {
    let accepted = source_cursor(&[("record-1", 'a'), ("record-2", 'b')]);
    assert_eq!(
        accepted
            .compare(&source_cursor(&[("record-1", 'a'), ("record-2", 'b')]))
            .expect("compare exact cursor"),
        SourceCursorComparison::Current
    );
    assert_eq!(
        accepted
            .compare(&source_cursor(&[
                ("record-1", 'a'),
                ("record-2", 'b'),
                ("record-3", 'c'),
            ]))
            .expect("compare appended cursor"),
        SourceCursorComparison::NewRecords(vec![SourceRecordDigest::new("record-3", digest('c'))])
    );
    assert!(matches!(
        accepted
            .compare(&source_cursor(&[("record-1", 'a')]))
            .expect("compare stale cursor"),
        SourceCursorComparison::Stale { .. }
    ));
    assert!(matches!(
        accepted
            .compare(&source_cursor(&[("record-1", 'a'), ("record-2", 'c')]))
            .expect("compare rewritten cursor"),
        SourceCursorComparison::Rewrite { .. }
    ));
    let different_session = SourceCursor::new(
        SourceProvider::Bundle,
        "other-private-session",
        digest('f'),
        vec![
            SourceRecordDigest::new("record-1", digest('a')),
            SourceRecordDigest::new("record-2", digest('b')),
        ],
    )
    .expect("build different-session cursor");
    assert_eq!(
        accepted
            .compare(&different_session)
            .expect("compare different session"),
        SourceCursorComparison::SessionMismatch
    );
}
