use kah_cli::commit_measure::decisive_selection::apply;
use serde_json::json;

#[test]
fn brain_selects_exact_complete_units_without_changing_authority_or_questions() {
    let body = json!({"model":"jev-1.13.0","state":{"checks":{"r":{"rule":"Never cut text"}},"current_permissions":{"p":"exact authority"},"diff_hunks":{"h1":{"file":"evidence.json","text":"whole first record\nwhole second record\n"}}},"questions":{"q":"unchanged"}});
    let text = body["state"]["diff_hunks"]["h1"]["text"].as_str().unwrap();
    let manifest = json!({"hunks":[{"id":"h1","source_digest":kah_observer::commit_check::digest(text.as_bytes()),"selected":[{"first_line":1,"last_line":1,"text":"whole first record\n","reason":"Complete decisive record","unit":"whole record"}],"excluded":[{"first_line":2,"last_line":2,"reason":"Unrelated second record"}]}]});
    let selected = apply(&body, &manifest).unwrap();
    assert_eq!(selected["questions"], body["questions"]);
    assert_eq!(selected["state"]["checks"], body["state"]["checks"]);
    assert_eq!(
        selected["state"]["current_permissions"],
        body["state"]["current_permissions"]
    );
    assert_eq!(
        selected["state"]["diff_hunks"]["h1"]["text"],
        "whole first record\n"
    );
    let mut invalid = manifest.clone();
    invalid["hunks"][0]["selected"][0]["text"] = json!("whole first");
    assert!(apply(&body, &invalid).is_err());
    invalid = manifest.clone();
    invalid["hunks"][0]["excluded"] = json!([]);
    assert!(apply(&body, &invalid).is_err());
}
