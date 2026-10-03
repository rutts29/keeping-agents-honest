use super::*;
use std::collections::BTreeSet;

pub fn validate_request(_state: &Value, questions: &Questions) -> Result<(), Error> {
    // Jev owns tokenization: 32k state + longest question, 64k total request.
    // Bytes and question counts are not token budgets. Preserve whole content;
    // provider rejection still needs the planned native-observer preparation;
    // it is not implemented by this validator.
    if questions.is_empty() {
        return Err(Error::new("JUDGE_QUESTIONS_EMPTY"));
    }
    for question in questions.values() {
        let valid = match question {
            Question::Noul { instructions } => !instructions.trim().is_empty(),
            Question::Choice {
                instructions,
                criteria,
            } => !instructions.trim().is_empty() && (2..=255).contains(&criteria.len()),
            Question::Score {
                instructions,
                criteria,
            } => !instructions.trim().is_empty() && (2..=10).contains(&criteria.len()),
        };
        if !valid {
            return Err(Error::new("JUDGE_QUESTION"));
        }
    }
    Ok(())
}

pub fn validate_response(
    questions: &Questions,
    answers: &BTreeMap<String, Answer>,
) -> Result<(), Error> {
    if !questions.keys().eq(answers.keys()) {
        return Err(Error::new("JUDGE_ANSWER_KEYS"));
    }
    for (id, question) in questions {
        match (question, &answers[id]) {
            (Question::Noul { .. }, Answer::Noul { noul }) if probability(*noul) => (),
            (
                Question::Choice { criteria, .. },
                Answer::Choice {
                    choice,
                    probabilities,
                    confidence,
                },
            ) => {
                distribution(
                    criteria.keys().cloned().collect(),
                    probabilities,
                    *confidence,
                )?;
                let chosen = probabilities
                    .get(choice)
                    .ok_or(Error::new("JUDGE_CHOICE"))?;
                if probabilities.values().any(|p| p > chosen) {
                    return Err(Error::new("JUDGE_CHOICE"));
                }
            }
            (
                Question::Score { criteria, .. },
                Answer::Score {
                    score,
                    legend,
                    probabilities,
                    confidence,
                },
            ) => {
                let expected = criteria
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i.to_string(), s.clone()))
                    .collect::<BTreeMap<_, _>>();
                if *legend != expected {
                    return Err(Error::new("JUDGE_SCORE_LEGEND"));
                }
                distribution(
                    expected.keys().cloned().collect(),
                    probabilities,
                    *confidence,
                )?;
                let mean = probabilities
                    .iter()
                    .map(|(level, p)| level.parse::<f64>().unwrap() * p)
                    .sum::<f64>();
                validate_score(*score, mean, criteria.len())?;
            }
            _ => return Err(Error::new("JUDGE_ANSWER_TYPE")),
        }
    }
    Ok(())
}
fn validate_score(score: f64, mean: f64, levels: usize) -> Result<(), Error> {
    // Jev's score uses the underlying distribution; returned probabilities are
    // rounded to two decimals. Each level contributes at most index * 0.005
    // to the recomputed mean's rounding error. This is a wire-format bound,
    // not a change to any judgment threshold.
    let tolerance = 0.005 * (0..levels).map(|index| index as f64).sum::<f64>() + 1e-9;
    if !score.is_finite() || (score - mean).abs() > tolerance {
        let mut error = Error::new("JUDGE_SCORE");
        error.numeric_detail = Some(format!(
            "reported_score={score} distribution_mean={mean} rounding_tolerance={tolerance}"
        ));
        return Err(error);
    }
    Ok(())
}
fn probability(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
fn distribution(
    expected: BTreeSet<String>,
    actual: &BTreeMap<String, f64>,
    confidence: f64,
) -> Result<(), Error> {
    if expected != actual.keys().cloned().collect()
        || !probability(confidence)
        || !actual.values().all(|p| probability(*p))
        || (actual.values().sum::<f64>() - 1.0).abs() > 0.02
    {
        return Err(Error::new("JUDGE_DISTRIBUTION"));
    }
    Ok(())
}
