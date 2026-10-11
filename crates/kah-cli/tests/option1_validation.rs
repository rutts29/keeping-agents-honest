use kah_cli::commit_measure::live::{Option1, candidate};
use kah_observer::judge::Answer;

#[test]
fn registered_data_line_is_inclusive_and_can_be_recalibrated_without_recoding() {
    let mut policy = Option1 {
        doubt_line: 0.5,
        estimate_usd: 0.03,
        authority: "Synthetic registered policy".into(),
    };
    assert!(!candidate(Some(&policy), &Answer::Noul { noul: 0.49 }));
    assert!(candidate(Some(&policy), &Answer::Noul { noul: 0.50 }));
    assert!(candidate(Some(&policy), &Answer::Noul { noul: 0.73 }));
    policy.doubt_line = 0.6;
    assert!(!candidate(Some(&policy), &Answer::Noul { noul: 0.50 }));
    assert!(candidate(Some(&policy), &Answer::Noul { noul: 0.60 }));
    assert!(candidate(None, &Answer::Noul { noul: 0.23 }));
}
