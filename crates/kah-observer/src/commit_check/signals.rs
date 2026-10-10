//! Signals are input for native confirmation, never feedback to the builder.
use super::*;
use crate::judge::Answer;
pub fn route(input: &Input, plan: &Plan, answers: &BTreeMap<String, Answer>) -> Value {
    let mut confirmation: Vec<_> = plan
        .correction_signals
        .iter()
        .filter(|s| s["action"] == "native_confirmation")
        .cloned()
        .collect();
    let unclear: Vec<_> = input
        .matrix
        .iter()
        .filter(|r| r.status == Status::Unclear)
        .map(|r| &r.requirement)
        .collect();
    for (id, query) in plan
        .literal
        .questions
        .iter()
        .chain(&plan.citations.questions)
    {
        let reason = match (query, answers.get(id)) {
            (Query::Noul { .. }, Some(Answer::Noul { noul })) if *noul <= 0.10 => None,
            (Query::Noul { .. }, Some(Answer::Noul { noul })) if *noul >= 0.90 => Some("rule_flag"),
            (
                Query::Choice { .. },
                Some(Answer::Choice {
                    choice,
                    probabilities,
                    confidence,
                }),
            ) if choice == "supports"
                && probabilities.get(choice).is_some_and(|p| *p >= 0.90)
                && *confidence >= 0.85 =>
            {
                None
            }
            (_, None) => Some("not_judged"),
            _ => Some("uncertain_or_unsupported"),
        };
        if let Some(reason) = reason {
            confirmation
                .push(json!({"question":id,"reason":reason,"action":"native_confirmation"}));
        }
    }
    for row in &input.matrix {
        if row.status == Status::Violated {
            confirmation.push(json!({"requirement":row.requirement,"reason":"matrix_flag","action":"native_confirmation"}));
        }
    }
    json!({"needs_native_confirmation":confirmation,"checkpoint_unclear":unclear,
        "unclear_diagnostics":plan.correction_signals.iter().filter(|s|s["action"]=="checkpoint_unclear").collect::<Vec<_>>(),"builder_delivery":false,"policy":"existing .90 probability/.85 Choice confidence; Noul has no confidence","raw_answers":answers})
}
