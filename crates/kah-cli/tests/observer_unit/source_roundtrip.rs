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
