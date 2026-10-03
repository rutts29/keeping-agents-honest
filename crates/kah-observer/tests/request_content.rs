use kah_observer::judge::{
    self, Answer, Judge, Question, Questions,
    jev::{Client, Jev},
};
use serde_json::{Value, json};

struct WholeRequest(Value);
impl Client for WholeRequest {
    fn post(&self, request: &Value) -> Result<Value, judge::Error> {
        assert_eq!(
            request["state"], self.0,
            "user words or evidence changed before the provider"
        );
        let answers = request["questions"]
            .as_object()
            .unwrap()
            .keys()
            .map(|key| (key.clone(), json!({"type":"noul","noul":0.99})))
            .collect::<serde_json::Map<_, _>>();
        Ok(
            json!({"model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":23000,"output_tokens":20}}),
        )
    }
}

#[test]
fn this_builders_real_ledger_and_whole_large_evidence_reach_jev_unchanged() {
    let ledger: Value =
        serde_json::from_str(include_str!("fixtures/builder-ledger-20261003.json")).unwrap();
    let evidence = format!(
        "{}\nLAST COMPLETE SENTENCE: keep all user words verbatim.",
        "A complete source observation with its original words.\n".repeat(1600)
    );
    let state = json!({"intent":ledger,"event":{"agent_text":evidence}});
    assert!(serde_json::to_vec(&state).unwrap().len() > 65536);
    let questions = Questions::from([(
        "whole_context".into(),
        Question::Noul {
            instructions: "Does the complete evidence follow the verbatim intent?".into(),
        },
    )]);
    let result = Jev {
        client: WholeRequest(state.clone()),
        model: "jev-latest".into(),
    }
    .evaluate(&state, &questions)
    .unwrap();
    assert!(matches!(result.answers["whole_context"],Answer::Noul{noul} if noul == 0.99));
}

#[test]
fn question_count_and_question_bytes_do_not_invent_a_provider_token_budget() {
    let state = json!({"task":"whole content"});
    let questions = (0..33)
        .map(|n| {
            (
                format!("q{n}"),
                Question::Noul {
                    instructions: format!(
                        "{} Does the full task apply?",
                        "Whole question instructions. ".repeat(20)
                    ),
                },
            )
        })
        .collect();
    assert!(serde_json::to_vec(&questions).unwrap().len() > 16384);
    Jev {
        client: WholeRequest(state.clone()),
        model: "jev-latest".into(),
    }
    .evaluate(&state, &questions)
    .unwrap();
}
