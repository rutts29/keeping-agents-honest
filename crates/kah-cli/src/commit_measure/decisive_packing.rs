//! Whole-file packing against documented token budgets, no content selection.
use super::*;
use kah_observer::commit_check::Prepared;
pub fn pack(original: &Prepared, calibration: &estimate::Calibration) -> Result<Vec<Prepared>> {
    if fits(original, calibration)? {
        return Ok(vec![original.clone()]);
    }
    let mut files: Vec<_> = original.state["diff_hunks"]
        .as_object()
        .unwrap()
        .iter()
        .collect();
    files.sort_by_key(|(id, file)| (std::cmp::Reverse(file.to_string().len()), *id));
    let mut groups: Vec<BTreeMap<String, Value>> = vec![];
    for (id, file) in files {
        let mut placed = false;
        for group in &mut groups {
            group.insert(id.clone(), file.clone());
            if fits(&body(original, group), calibration)? {
                placed = true;
                break;
            }
            group.remove(id);
        }
        if !placed {
            groups.push(BTreeMap::from([(id.clone(), file.clone())]));
        }
    }
    Ok(groups.iter().map(|g| body(original, g)).collect())
}
fn body(original: &Prepared, files: &BTreeMap<String, Value>) -> Prepared {
    let mut state = original.state.clone();
    state["diff_hunks"] = json!(files);
    for check in state["checks"].as_object_mut().unwrap().values_mut() {
        check["hunk_ids"] = json!(
            check["hunk_ids"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|id| files.contains_key(id.as_str().unwrap()))
                .collect::<Vec<_>>()
        );
    }
    Prepared {
        state,
        questions: original.questions.clone(),
    }
}
pub fn fits(body: &Prepared, calibration: &estimate::Calibration) -> Result<bool> {
    let longest = body
        .questions
        .values()
        .map(|q| match q {
            commit_check::Query::Noul { instructions } => instructions.len(),
            _ => 0,
        })
        .max()
        .unwrap_or(0);
    let state = serde_json::to_vec(&body.state)?.len() + longest;
    Ok(calibration.estimate(state as u64)["empirical_input_tokens"]
        .as_f64()
        .unwrap()
        <= 32000.0
        && calibration.estimate(body.body()?.len() as u64)["empirical_input_tokens"]
            .as_f64()
            .unwrap()
            <= 64000.0)
}
