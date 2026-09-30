use super::*;
#[test]
fn historical_questions_use_incremental_no_change_instead_of_initial_requirements() {
    let bundle = NeutralSessionBundle::from_records(
        SourceProvider::Codex,
        "private-session",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        vec![
            BundleRecord::new("u1", SourceRole::User, "Ship the inspector").unwrap(),
            BundleRecord::new("question", SourceRole::User, "Can you explain the issue?").unwrap(),
            BundleRecord::new("u3", SourceRole::User, "Keep the output concise").unwrap(),
        ],
    )
    .unwrap();
    let chunks = chunking::initial_chunks(&bundle).unwrap();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[1].authoritative_records()[0].id(), "question");
    let (history, projection) = accepted_state();
    let response = proposal(&history, &projection, "no_change", "question", json!([]));
    assert!(
        validate_incremental_and_project(
            &history,
            &projection,
            &chunks[1],
            response,
            ProjectionConfig::default()
        )
        .is_ok()
    );
}
