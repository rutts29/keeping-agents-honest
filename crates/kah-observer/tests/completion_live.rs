use kah_observer::judge::{
    self, Answer, Judge,
    jev::{HttpClient, Jev},
};
use serde_json::json;
use std::{fs, path::PathBuf};
#[test]
#[ignore = "live Jev positive and negative completion judgments; gate only"]
fn completion_distinguishes_satisfied_violated_and_missing_evidence() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let judge = Jev {
        client: HttpClient::from_home(&home).unwrap(),
        model: "jev-latest".into(),
    };
    let outcomes = vec![(
        "greeting".into(),
        "The program prints BETA followed by a newline.".into(),
    )];
    let questions = judge::questions::completion(&outcomes);
    let mut reports = vec![];
    for (marker, expected) in [("BETA", "satisfied"), ("ALPHA", "violated")] {
        let state = json!({"intent":{"goal":"Make greet.c print BETA followed by a newline.","outcomes":outcomes},"event":{"actions":{
            "committed_files":[{"path":"greet.c","complete":true,"content":format!("#include <stdio.h>\nint main(void) {{ puts(\"{marker}\"); return 0; }}\n")}],
            "tests":{"actual_stdout":format!("{marker}\n"),"expected_stdout":"BETA\n","success":marker=="BETA"}
        }}});
        let result = judge.evaluate(&state, &questions).unwrap();
        assert!(
            matches!(&result.answers["outcome_met.greeting"], Answer::Choice{choice,..} if choice==expected)
        );
        assert_eq!(
            judge::decision::complete(&result, &outcomes),
            marker == "BETA"
        );
        reports.push(json!({"expected":expected,"judgment":result}));
    }
    let missing = judge.evaluate(&json!({"intent":{"goal":"Make greet.c print BETA followed by a newline.","outcomes":outcomes},"event":{"actions":{}}}), &questions).unwrap();
    assert!(
        matches!(&missing.answers["outcome_met.greeting"], Answer::Choice{choice,..} if choice=="unclear")
    );
    assert!(!judge::decision::complete(&missing, &outcomes));
    reports.push(json!({"expected":"unclear","judgment":missing}));
    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/gates/evidence/phase-4/completion-contract.json"),
        serde_json::to_string_pretty(
            &json!({"questionSet":judge::questions::VERSION,"reports":reports,"passed":true}),
        )
        .unwrap()
            + "\n",
    )
    .unwrap();
}
