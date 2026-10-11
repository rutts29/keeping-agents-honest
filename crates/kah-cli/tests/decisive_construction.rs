use kah_cli::commit_measure::{decisive_bookkeeping, decisive_packing, estimate::Calibration};
use kah_observer::commit_check::{Prepared, Query};
use serde_json::json;
use std::collections::BTreeMap;
fn fixture() -> Prepared {
    let name = "item_abcdefghijklmnopqrstuvwxyz";
    let rule_id = format!("user-{}-rule-002", "f11d18e5".to_owned() + &"a".repeat(56));
    Prepared {
        state: json!({"diff_hunks":{"original_hunk":{"file":"a.rs","text":"whole changed file\n"},"second_hunk":{"file":"b.rs","text":"second whole file\n"}},"checks":{name:{"requirement":rule_id,"rule":"Never impose KB caps anywhere.","rule_origin":"user","source_ids":[{"id":"full source identity","digest":"exact original digest"}],"hunk_ids":["original_hunk","second_hunk"],"permission_ids":["p"]}},"current_permissions":{"p":{"text":"exact user permission context"}}}),
        questions: BTreeMap::from([(
            format!("rule.{name}"),
            Query::Noul {
                instructions: format!(
                    "Read `checks.{name}.rule`, `checks.{name}.hunk_ids` and `checks.{name}.permission_ids` verbatim."
                ),
            },
        )]),
    }
}
#[test]
fn short_names_preserve_all_words_and_context_while_audit_identity_stays_local() {
    let original = fixture();
    let (short, map) = decisive_bookkeeping::literal(&original).unwrap();
    let check = &short.state["checks"]["r_f11d18e5_002"];
    assert_eq!(
        check
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["hunk_ids", "permission_ids", "rule"]
    );
    assert_eq!(
        check["rule"],
        original.state["checks"]["item_abcdefghijklmnopqrstuvwxyz"]["rule"]
    );
    assert_eq!(
        short.state["current_permissions"],
        original.state["current_permissions"]
    );
    let Query::Noul { instructions } = &short.questions["rule.r_f11d18e5_002"] else {
        panic!()
    };
    let Query::Noul { instructions: old } =
        &original.questions["rule.item_abcdefghijklmnopqrstuvwxyz"]
    else {
        panic!()
    };
    assert_eq!(
        instructions.replace("r_f11d18e5_002", "item_abcdefghijklmnopqrstuvwxyz"),
        *old
    );
    assert_eq!(
        map["rules"]["r_f11d18e5_002"]["source_ids"],
        original.state["checks"]["item_abcdefghijklmnopqrstuvwxyz"]["source_ids"]
    );
    assert!(!short.body().unwrap().contains("full source identity"));
}
#[test]
fn fitting_commit_is_one_body_with_every_file_and_identical_questions() {
    let (original, _) = decisive_bookkeeping::literal(&fixture()).unwrap();
    let calibration = Calibration {
        samples: 1,
        input_tokens: 1,
        input_utf8_bytes: 1,
        minimum_ratio: 1.0,
        maximum_ratio: 1.0,
        source_digests: BTreeMap::new(),
    };
    let packed = decisive_packing::pack(&original, &calibration).unwrap();
    assert_eq!(packed.len(), 1);
    assert_eq!(packed[0].body().unwrap(), original.body().unwrap());
}
#[test]
fn oversized_file_is_kept_whole_and_never_cut_or_silently_omitted() {
    let (mut original, _) = decisive_bookkeeping::literal(&fixture()).unwrap();
    original.state["diff_hunks"]["h1"]["text"] = json!("complete record\n".repeat(4000));
    let calibration = Calibration {
        samples: 1,
        input_tokens: 1,
        input_utf8_bytes: 1,
        minimum_ratio: 1.0,
        maximum_ratio: 1.0,
        source_digests: BTreeMap::new(),
    };
    let packed = decisive_packing::pack(&original, &calibration).unwrap();
    assert_eq!(packed.len(), 2);
    let files: BTreeMap<_, _> = packed
        .iter()
        .flat_map(|b| b.state["diff_hunks"].as_object().unwrap())
        .map(|(id, v)| (id.clone(), v.clone()))
        .collect();
    assert_eq!(
        serde_json::to_value(files).unwrap(),
        original.state["diff_hunks"]
    );
    assert!(
        packed
            .iter()
            .any(|b| !decisive_packing::fits(b, &calibration).unwrap())
    );
    for body in packed {
        assert_eq!(
            serde_json::to_value(body.questions).unwrap(),
            serde_json::to_value(&original.questions).unwrap()
        );
    }
}
