//! Explicit offline measurement; does not construct or invoke any Judge client.
use crate::Result;
use kah_observer::commit_check::{self, Input, Plan};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
pub mod comparison;
pub mod confirmation;
pub mod historical;
pub mod replay;
pub mod retained;
mod source;
pub use source::collect;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub records: Vec<kah_core::intent_history::EventRecord>,
    #[serde(default)]
    pub historical: Option<historical::Snapshot>,
    pub quality: BTreeMap<String, commit_check::Authority>,
    pub input: Input,
    pub matrix_source: String,
    pub native_usage: Option<Value>,
    #[serde(default)]
    pub calibration: Option<estimate::Calibration>,
}
pub fn resolve(bundle: &Bundle) -> Result<Input> {
    let mut input = bundle.input.clone();
    input.intent = if let Some(snapshot) = &bundle.historical {
        if !bundle.records.is_empty() || !bundle.quality.is_empty() {
            return Err("MEASURE_HISTORICAL_AUTHORITY stage=resolve: historical snapshots cannot mix injected records or quality rules".into());
        }
        snapshot.intent()?
    } else {
        let history = kah_core::intent_history::History::replay(bundle.records.clone())?;
        commit_check::CurrentIntent::resolve(&history, input.intent.permissions.clone())?
    };
    for (id, item) in &bundle.quality {
        if item.origin != "quality" || input.intent.items.contains_key(id) {
            return Err(format!("MEASURE_QUALITY_AUTHORITY stage=resolve: {id}").into());
        }
        input.intent.items.insert(id.clone(), item.clone());
    }
    literal_typing(bundle, &mut input)?;
    Ok(input)
}
pub fn prepare(bundle: &Bundle) -> Result<Plan> {
    commit_check::prepare(&resolve(bundle)?).map_err(Into::into)
}
pub fn count(bundle: &Bundle, output: &Path) -> Result<Value> {
    let plan = prepare(bundle)?;
    kah_core::run_store::ensure_private_directory(output)?;
    let mut measurements = vec![];
    for (stage, request) in [("literal", &plan.literal), ("citation", &plan.citations)] {
        if request.questions.is_empty() {
            continue;
        }
        let body = request.body()?;
        save_text(output, &format!("{stage}.json"), &body)?;
        measurements.push(json!({"stage":stage,"request_utf8_bytes":body.len(),
            "state_utf8_bytes":serde_json::to_vec(&request.state)?.len(),"questions":request.questions.len(),
            "digest":commit_check::digest(body.as_bytes()),"estimate":bundle.calibration.as_ref().map(|c|c.estimate(body.len() as u64)),"choice_order":["says_nothing","contradicts","supports"]}));
    }
    save_text(output, "matrix-prompt.txt", &plan.matrix_prompt)?;
    save_text(output, "input.json", &serde_json::to_string(bundle)?)?;
    save_text(
        output,
        "matrix-input.json",
        &serde_json::to_string(&resolve(bundle)?)?,
    )?;
    let report = json!({"mode":"count_only","revision":plan.revision,"requests":measurements,
        "touched":plan.touched,"carried":plan.carried,"matrix_pending":plan.matrix_pending,
        "absence_rows":plan.absence_rows,"correction_signals":plan.correction_signals,
        "matrix_source":bundle.matrix_source,"native_usage":bundle.native_usage,
        "rule_count":resolve(bundle)?.rules.values().filter(|r|r.literal_check).count(),
        "matrix_requirement_count":plan.touched.len(),"literal_typing":"Stored intake IntentKind Constraint/Invariant; Goal/Outcome/Scope/Validation/StopCondition are matrix requirements",
        "authority_item_count":bundle.input.intent.items.len(),
        "historical_authority":bundle.historical.as_ref().map(|s| &s.audit),
        "actual_jev_calls":0,"actual_jev_input_tokens":0,"actual_jev_cost_usd":0.0,
        "planned_jev_input_tokens":null,"planned_jev_cost_usd":null,
        "estimate_note":"No official tokenizer. Exact serialized sizes are measured; calibrated estimates can be attached separately, never used as caps.",
        "quality_judgment":"not_run","builder_delivery":false,"cost_is_pass_fail":false});
    save_text(
        output,
        "count.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    Ok(report)
}
fn save_text(root: &Path, name: &str, text: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(root.join(name))?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(())
}
pub fn command(args: &[String]) -> Result<()> {
    match args {
        [mode, input, output] if mode == "count" => {
            let bundle: Bundle = serde_json::from_slice(&fs::read(input)?)?;
            println!("{}", count(&bundle, Path::new(output))?);
        }
        [mode, input, matrix, output] if mode == "matrix" => {
            let mut bundle: Bundle = serde_json::from_slice(&fs::read(input)?)?;
            bundle.input.matrix = serde_json::from_slice(&fs::read(matrix)?)?;
            bundle.matrix_source = format!("native:{matrix}");
            println!("{}", count(&bundle, Path::new(output))?);
        }
        [mode,input,answers,output] if mode == "confirmation" => {
            let bundle:Bundle=serde_json::from_slice(&fs::read(input)?)?;
            let answers:confirmation::Answers=serde_json::from_slice(&fs::read(answers)?)?;
            println!("{}",confirmation::count(&bundle,&answers,Path::new(output))?);
        }
        [mode, repo, labels, output] if mode == "replay" => replay::run(Path::new(repo), Path::new(labels), Path::new(output))?,
        _ => return Err("MEASURE_ARGUMENTS stage=cli: kah measure count INPUT OUTPUT | matrix INPUT MATRIX OUTPUT | confirmation INPUT ANSWERS OUTPUT | replay REPO LABELS OUTPUT".into()),
    }
    Ok(())
}

pub mod corpus;

pub mod estimate;

pub mod step_c;

fn literal_typing(bundle: &Bundle, input: &mut Input) -> Result<()> {
    use kah_core::intent_history::{History, IntentKind};
    let ids = if let Some(snapshot) = &bundle.historical {
        snapshot
            .projection
            .preserve
            .iter()
            .chain(&snapshot.projection.frontier)
            .chain(&snapshot.projection.validation)
            .filter(|i| matches!(i.kind, IntentKind::Constraint | IntentKind::Invariant))
            .map(|i| i.id.to_string())
            .collect::<std::collections::BTreeSet<_>>()
    } else {
        History::replay(bundle.records.clone())?
            .snapshot()
            .intents
            .values()
            .filter(|i| {
                i.is_active() && matches!(i.kind, IntentKind::Constraint | IntentKind::Invariant)
            })
            .map(|i| i.id.to_string())
            .collect()
    };
    for rule in input.rules.values_mut() {
        rule.literal_check = ids.contains(&rule.authority);
        rule.unconditional_prohibition = rule.literal_check;
        rule.prohibition = rule.literal_check;
    }
    Ok(())
}
