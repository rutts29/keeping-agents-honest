use super::*;

#[test]
fn streamed_comparison_rejects_changed_text_shape_and_scalar_without_a_full_mirror() {
    let root = std::env::temp_dir().join(format!("compare-source-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let original = json!({"state":{"text":"Whole words 🦀. ".repeat(8000),"values":[true,null,3]},"documents":[{"text":"Complete source."}]});
    let stored = compact(&original, &root).unwrap();
    assert!(matches(&stored, &original, &root).unwrap());
    for changed in [
        json!({"state":{"text":"different","values":[true,null,3]},"documents":[{"text":"Complete source."}]}),
        json!({"state":{"text":original["state"]["text"],"values":[false,null,3]},"documents":[{"text":"Complete source."}]}),
        json!({"state":original["state"],"documents":[]}),
        json!({"state":original["state"],"documents":original["documents"],"extra":true}),
    ] {
        assert!(!matches(&stored, &changed, &root).unwrap());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn jev_estimated_costs_survive_exact_json_source_roundtrips() {
    // serde_json v1.0.151 src/de.rs::f64_from_parts uses its native
    // float_roundtrip feature for correctly rounded parsing; no tolerance.
    let root = std::env::temp_dir().join(format!("cost-roundtrip-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    for input_tokens in [486, 488, 550, 556, 560, 574, 584, 586, 600, 681] {
        let cost = input_tokens as f64 * 0.042 / 1_000_000.0;
        let original =
            json!({"state":{"metrics":{"input_tokens":input_tokens,"estimated_cost_usd":cost}}});
        let stored = compact(&original, &root).unwrap();
        assert!(
            matches(&stored, &original, &root).unwrap(),
            "estimated cost changed for {input_tokens} input tokens"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
