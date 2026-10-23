//! Retained-data mechanics only. Fixtures deliberately do not establish relevance.
use kah_observer::{context_budget::Calibration, context_selection::digest, whole_preparation::*};
use serde_json::{Value, json};
mod corpus;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};
#[test]
#[ignore = "seven retained actual rejects; explicit output path; zero native/provider inference"]
fn retained_whole_records_prove_mechanics_not_selection_quality() {
    let mut results = vec![];
    let mut budget_controls = 0;
    for entry in fs::read_dir(corpus::path("preparation/jev")).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("response-")
        {
            continue;
        }
        let response: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        if response["error_response"]["status"] != 400 {
            continue;
        }
        let path = corpus::locator(response["job"]["path"].as_str().unwrap());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            response["job"]["digest"],
            format!("sha256:{:x}", Sha256::digest(&bytes))
        );
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        budget_controls += usize::from(response["job"]["request_budget_control"] == true);
        results.push(exercise(&body, response["job"].clone()));
        assert_eq!(bytes, fs::read(&path).unwrap());
    }
    assert_eq!(results.len(), 7);
    assert_eq!(budget_controls, 1);
    results.sort_by_key(|v| v["job"]["path"].as_str().unwrap().to_owned());
    let output = std::env::var_os("KAH_WHOLE_RECORD_OUTPUT").unwrap();
    let output = corpus::fresh_output(Path::new(&output), &[&corpus::root()]);
    fs::write(output,serde_json::to_vec_pretty(&json!({"cases":results,"provider_calls":0,"native_calls":0,
        "selection_quality_proven":false,"provider_acceptance_proven":false,
        "fixture":"metadata-only is intentionally incomplete; fit result tests mechanics, never authorizes a send"})).unwrap()).unwrap();
}
fn exercise(body: &Value, job: Value) -> Value {
    let catalogue = catalogue(body);
    let selected = manifest(&catalogue, false);
    let mut policy = Policy {
        calibration: Some(Calibration {
            input_tokens: 71000,
            input_utf8_bytes: 180268,
        }),
        known_oversized: BTreeSet::from([digest(body)]),
    };
    let keep_all = prepare(body, &catalogue, &manifest(&catalogue, true), &policy).unwrap();
    assert!(
        matches!(&keep_all,Outcome::DirectObserverReview{gap} if gap.reason=="known_oversized")
    );
    let fitted = prepare(body, &catalogue, &selected, &policy).unwrap();
    let Outcome::Rendered {
        body: rendered,
        estimate,
        body_digest,
    } = fitted
    else {
        panic!("fixture must predict fit")
    };
    assert!(!estimate.prepare && estimate.planning_only);
    assert_eq!(rendered["questions"], body["questions"]);
    assert_eq!(rendered["state"]["checks"], body["state"]["checks"]);
    assert_eq!(
        rendered["state"]["current_permissions"],
        body["state"]["current_permissions"]
    );
    for source in &catalogue.sources {
        let values: Vec<Value> =
            serde_json::from_str(rendered.pointer(&source.target).unwrap().as_str().unwrap())
                .unwrap();
        assert_eq!(values, vec![source.document["file"].clone()]);
    }
    invalid_controls(body, &catalogue, &selected, &policy);
    let mut reordered = selected.clone();
    reordered.selected.reverse();
    reordered.excluded.reverse();
    assert_eq!(
        serde_json::to_value(prepare(body, &catalogue, &reordered, &policy).unwrap()).unwrap(),
        serde_json::to_value(prepare(body, &catalogue, &selected, &policy).unwrap()).unwrap()
    );
    policy.known_oversized.insert(body_digest);
    assert!(
        matches!(prepare(body,&catalogue,&selected,&policy).unwrap(),Outcome::DirectObserverReview{gap} if gap.reason=="known_oversized")
    );
    assert!(
        matches!(prepare(body,&catalogue,&selected,&Policy::default()).unwrap(),Outcome::DirectObserverReview{gap} if gap.reason=="calibration_unavailable")
    );
    if job["request_budget_control"] == true {
        request_budget_control(body, &catalogue, &selected);
    }
    json!({"job":job,"keep_all":keep_all,"mechanics_only_fit":estimate,"deterministic":true,"invalid_controls_rejected":true})
}
fn catalogue(body: &Value) -> Catalogue {
    let sources = body["state"]["diff_hunks"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, hunk)| {
            assert_eq!(hunk.as_object().unwrap().len(), 2);
            let records = ["file", "text"]
                .into_iter()
                .map(|key| Record {
                    id: format!("{id}:{key}"),
                    pointer: format!("/{key}"),
                    digest: digest(&hunk[key]),
                    required: key == "file",
                    dependencies: if key == "text" {
                        BTreeSet::from([format!("{id}:file")])
                    } else {
                        BTreeSet::new()
                    },
                })
                .collect();
            Source {
                target: format!("/state/diff_hunks/{id}/text"),
                original_digest: digest(&hunk["text"]),
                document: hunk.clone(),
                document_digest: digest(hunk),
                records,
            }
        })
        .collect();
    Catalogue {
        body_digest: digest(body),
        sources,
    }
}
fn manifest(catalogue: &Catalogue, all: bool) -> Manifest {
    let choices = catalogue
        .sources
        .iter()
        .flat_map(|s| s.records.iter())
        .map(|r| {
            (
                r,
                Choice {
                    id: r.id.clone(),
                    digest: r.digest.clone(),
                    reason: "mechanics fixture; relevance intentionally not asserted".into(),
                },
            )
        })
        .collect::<Vec<_>>();
    Manifest {
        body_digest: catalogue.body_digest.clone(),
        catalogue_digest: catalogue_digest(catalogue),
        selected: choices
            .iter()
            .filter(|(r, _)| all || r.required)
            .map(|(_, c)| c.clone())
            .collect(),
        excluded: choices
            .iter()
            .filter(|(r, _)| !all && !r.required)
            .map(|(_, c)| c.clone())
            .collect(),
    }
}
fn invalid_controls(body: &Value, catalogue: &Catalogue, selected: &Manifest, policy: &Policy) {
    let mut missing = selected.clone();
    missing.excluded.pop();
    assert!(
        prepare(body, catalogue, &missing, policy)
            .unwrap_err()
            .starts_with("PREPARATION_UNACCOUNTED")
    );
    let mut source = catalogue.clone();
    source.sources[0].document["text"] = "substituted".into();
    assert!(
        prepare(body, &source, selected, policy)
            .unwrap_err()
            .starts_with("PREPARATION_SOURCE_BINDING")
    );
    let mut optional = catalogue.clone();
    optional.sources[0].records[0].required = false;
    let mut dependency = manifest(&optional, true);
    let context = dependency.selected.remove(0);
    dependency.excluded.push(context);
    assert!(
        prepare(body, &optional, &dependency, policy)
            .unwrap_err()
            .starts_with("PREPARATION_DEPENDENCY")
    );
}
fn request_budget_control(body: &Value, catalogue: &Catalogue, manifest: &Manifest) {
    let policy = Policy {
        calibration: Some(Calibration {
            input_tokens: 11,
            input_utf8_bytes: 20,
        }),
        known_oversized: BTreeSet::new(),
    };
    let Outcome::DirectObserverReview { gap } =
        prepare(body, catalogue, manifest, &policy).unwrap()
    else {
        panic!("request budget must also be enforced")
    };
    let estimate = gap.estimate.unwrap();
    assert!(estimate.estimated_state_and_longest_tokens < 32000.0);
    assert!(estimate.estimated_request_tokens > 64000.0);
}
