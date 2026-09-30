use super::*;

fn failure(status: u16) -> Error {
    Error {
        code: "JEV_HTTP_STATUS".into(),
        stage: "judge_http",
        status: Some(status),
        numeric_detail: None,
    }
}
#[test]
fn transient_errors_back_off_and_never_fall_back_to_another_backend() {
    let mut calls = 0;
    let mut delays = vec![];
    let judgment = retry(
        || {
            calls += 1;
            if calls < 3 {
                return Err(failure(if calls == 1 { 429 } else { 529 }));
            }
            Ok(Judgment {
                backend: "jev".into(),
                model: "jev-test".into(),
                answers: BTreeMap::new(),
                metrics: Metrics::default(),
                fallback_reason: None,
            })
        },
        |delay| delays.push(delay.as_millis()),
    )
    .unwrap();
    assert_eq!(calls, 3);
    assert_eq!(delays, [250, 1000]);
    assert_eq!(judgment.metrics.failed_calls, 2);
    assert_eq!(judgment.metrics.calls, 3);
    let mut calls = 0;
    let error = retry(
        || {
            calls += 1;
            Err(failure(529))
        },
        |_| {},
    )
    .unwrap_err();
    assert_eq!(error.status, Some(529));
    assert_eq!(calls, 3);
    let mut calls = 0;
    assert!(
        retry(
            || {
                calls += 1;
                Err(failure(401))
            },
            |_| panic!("permanent error retried")
        )
        .is_err()
    );
    assert_eq!(calls, 1);
    for domain in ["mine", "research", "untrusted"] {
        assert_eq!(Backend::for_domain(domain, None).unwrap(), Backend::Jev);
        assert!(Backend::for_domain(domain, Some("luna")).is_err());
    }
}
