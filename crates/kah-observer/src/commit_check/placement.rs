//! Adopted after frozen31-case b3853d8 replay: no unrelated rule-state hops.
use super::*;
pub fn rule_in_question(original: &Prepared) -> Result<Prepared, String> {
    let mut result = original.clone();
    let checks = result.state["checks"]
        .as_object_mut()
        .ok_or("MEASURE_RULE_PLACEMENT stage=request: checks missing")?;
    for (key, check) in checks {
        let reference = format!("`checks.{key}.rule`");
        let Some(Query::Noul { instructions }) = result.questions.get_mut(&format!("rule.{key}"))
        else {
            return Err("MEASURE_RULE_PLACEMENT stage=request: matching Noul missing".into());
        };
        if instructions.matches(&reference).count() != 1 {
            return Err(
                "MEASURE_RULE_PLACEMENT stage=request: exact single rule reference required".into(),
            );
        }
        let words = check
            .as_object_mut()
            .and_then(|c| c.remove("rule"))
            .filter(Value::is_string)
            .ok_or("MEASURE_RULE_PLACEMENT stage=request: complete rule string required")?;
        *instructions = instructions.replace(&reference, &format!("the rule {words}"));
    }
    Ok(result)
}
