use super::*;
use crate::judge::Answer;

#[derive(Default)]
struct Count {
    total: u64,
    correct: u64,
    abstained: u64,
}
impl Count {
    fn add(&mut self, expected: &str, actual: Option<&str>) {
        self.total += 1;
        self.correct += u64::from(actual == Some(expected));
        self.abstained += u64::from(actual.is_none());
    }
    fn report(&self) -> Value {
        json!({"total":self.total,"correct":self.correct,"abstained":self.abstained,"accuracy":ratio(self.correct,self.total),"accuracyWhenAnswered":ratio(self.correct,self.total-self.abstained)})
    }
}
#[derive(Default)]
pub struct Scores {
    accuracy: BTreeMap<String, Count>,
    confusion: BTreeMap<String, BTreeMap<String, BTreeMap<String, u64>>>,
    tags: BTreeMap<String, BTreeMap<String, Count>>,
    sweeps: BTreeMap<String, Count>,
    rows: u64,
    calls: u64,
    failed_calls: u64,
    input: u64,
    output: u64,
    elapsed: u64,
    cost: f64,
    errors: BTreeMap<String, u64>,
    latency: Vec<u64>,
    models: BTreeMap<String, u64>,
    protected_goal_selections: u64,
}
impl Scores {
    pub fn add(&mut self, case: &Case, record: &Record) -> bool {
        self.rows += 1;
        self.usage(record);
        let p = probability(record);
        let steering = p
            .map(|p| if p >= 0.5 { "true" } else { "false" })
            .or_else(|| (record.source != "user").then_some("false"));
        let kind = choice(record, "steer_kind");
        let target = choice(record, "supersedes").or_else(|| {
            (!case.questions.contains_key("supersedes") && record.error.is_none()).then_some("none")
        });
        let labels = [
            (
                "is_steering",
                Some(if case.row.gold.is_steering {
                    "true"
                } else {
                    "false"
                }),
                steering,
            ),
            ("steer_kind", Some(case.row.gold.steer_kind.as_str()), kind),
            ("supersedes", case.row.gold.supersedes.as_deref(), target),
        ];
        let mut correct = true;
        for (question, expected, actual) in labels {
            if let Some(expected) = expected {
                self.score(case, question, expected, actual);
                correct &= actual == Some(expected);
            }
        }
        self.thresholds(case, record, p);
        if target.is_some_and(|t| t != "none" && case.state["prior_intent"]["goal_item_id"] == t)
            && kind != Some("goal_change")
        {
            self.protected_goal_selections += 1;
        }
        correct
    }
    fn score(&mut self, case: &Case, question: &str, expected: &str, actual: Option<&str>) {
        self.accuracy
            .entry(question.into())
            .or_default()
            .add(expected, actual);
        *self
            .confusion
            .entry(question.into())
            .or_default()
            .entry(expected.into())
            .or_default()
            .entry(actual.unwrap_or("__abstained").into())
            .or_default() += 1;
        let mut tags = case.row.gold.tags.clone();
        if tags.iter().any(|t| t == "approval") && case.row.user_text.chars().count() <= 80 {
            tags.push("terse-approval".into());
        }
        for tag in &tags {
            self.tags
                .entry(tag.clone())
                .or_default()
                .entry(question.into())
                .or_default()
                .add(expected, actual);
        }
    }
    fn thresholds(&mut self, case: &Case, record: &Record, p: Option<f64>) {
        for threshold in [0.5, 0.6, 0.7, 0.8, 0.85, 0.9, 0.95] {
            let actual = p.map(|p| if p >= threshold { "true" } else { "false" });
            self.sweeps
                .entry(format!("is_steering_probability_{threshold:.2}"))
                .or_default()
                .add(
                    if case.row.gold.is_steering {
                        "true"
                    } else {
                        "false"
                    },
                    actual,
                );
            for question in ["steer_kind", "supersedes"] {
                let expected = if question == "steer_kind" {
                    Some(case.row.gold.steer_kind.as_str())
                } else {
                    case.row.gold.supersedes.as_deref()
                };
                if let Some(expected) = expected {
                    let actual = record
                        .judgment
                        .as_ref()
                        .and_then(|j| j.answers.get(question))
                        .and_then(|a| match a {
                            Answer::Choice {
                                choice,
                                confidence,
                                probabilities,
                            } if *confidence >= threshold
                                && probabilities.get(choice).is_some_and(|p| *p >= threshold) =>
                            {
                                Some(choice.as_str())
                            }
                            _ => None,
                        });
                    self.sweeps
                        .entry(format!(
                            "{question}_probability_and_confidence_{threshold:.2}"
                        ))
                        .or_default()
                        .add(expected, actual);
                }
            }
        }
    }
    fn usage(&mut self, record: &Record) {
        if let Some(error) = &record.error {
            *self.errors.entry(error.clone()).or_default() += 1;
        }
        if let Some(j) = &record.judgment {
            self.calls += j.metrics.calls;
            self.failed_calls += j.metrics.failed_calls;
            self.input += j.metrics.input_tokens;
            self.output += j.metrics.output_tokens;
            self.elapsed += j.metrics.elapsed_ms;
            self.cost += j.metrics.estimated_cost_usd.unwrap_or(0.0);
            self.latency.push(j.metrics.elapsed_ms);
            *self.models.entry(j.model.clone()).or_default() += 1;
        }
    }
    pub fn report(&self) -> Value {
        let mut latency = self.latency.clone();
        latency.sort();
        let map = |v: &BTreeMap<String, Count>| {
            v.iter()
                .map(|(k, c)| (k.clone(), c.report()))
                .collect::<BTreeMap<_, _>>()
        };
        json!({"rows":self.rows,"binaryAccuracyThreshold":0.5,"perQuestion":map(&self.accuracy),"confusion":self.confusion,"byTag":self.tags.iter().map(|(t,v)|(t.clone(),map(v))).collect::<BTreeMap<_,_>>(),"thresholdSweeps":map(&self.sweeps),"thresholdsAutomaticallyChanged":false,"protectedGoalSelectionsBlocked":self.protected_goal_selections,"errors":self.errors,"models":self.models,"metrics":{"callsRepresented":self.calls,"failedCalls":self.failed_calls,"inputTokens":self.input,"outputTokens":self.output,"elapsedMs":self.elapsed,"knownEstimatedCostUsd":self.cost,"usageIsLowerBound":!self.errors.is_empty() || self.failed_calls>0,"p50Ms":quantile(&latency,50),"p95Ms":quantile(&latency,95)}})
    }
}
fn ratio(n: u64, d: u64) -> Option<f64> {
    (d > 0).then(|| n as f64 / d as f64)
}
fn quantile(v: &[u64], pct: usize) -> Option<u64> {
    v.get((v.len() * pct).div_ceil(100).saturating_sub(1))
        .copied()
}
fn probability(record: &Record) -> Option<f64> {
    match record.judgment.as_ref()?.answers.get("is_steering")? {
        Answer::Noul { noul } => Some(*noul),
        _ => None,
    }
}
fn choice<'a>(record: &'a Record, key: &str) -> Option<&'a str> {
    if record.source != "user" {
        return Some("none");
    }
    match record.judgment.as_ref()?.answers.get(key)? {
        Answer::Choice { choice, .. } => Some(choice),
        _ => None,
    }
}
