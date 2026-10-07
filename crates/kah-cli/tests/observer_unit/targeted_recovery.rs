use super::*;
#[test]
fn completed_targeted_receipt_is_recovered_and_binds_command_sha_and_side_effects() {
    let root = std::env::temp_dir().join(format!(
        "kah-targeted-recovery-{}",
        crate::relay_store::nonce()
    ));
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    let item = Item {
        requirement_id: "normalize".into(),
        claim: "Rounding map".into(),
        evidence_citations: vec![0],
        coverage_reasoning: "Exercise export.".into(),
        method: Method::TargetedCheck {
            command: "node check.cjs".into(),
        },
    };
    let head = "a".repeat(40);
    let path = crate::exact_verification::receipt_path(&root.join("normalize.log"));
    let value = json!({"receipt":{"command":"node check.cjs","commit":head,"success":true,"exit_code":0,"elapsed_ms":1,"output":"NATIVE_CHECK_PASS\n","output_truncated":false},"changed":false});
    crate::unattended_report::save(&root, path.file_name().unwrap().to_str().unwrap(), &value)
        .unwrap();
    let check = recovered_target(&root, &item, "node check.cjs", &head)
        .unwrap()
        .unwrap();
    assert!(check.success);
    assert_eq!(check.output, "NATIVE_CHECK_PASS\n");
    assert!(recovered_target(&root, &item, "other command", &head).is_err());
    assert!(recovered_target(&root, &item, "node check.cjs", &"b".repeat(40)).is_err());
    let mut changed = value;
    changed["changed"] = true.into();
    crate::unattended_report::save(&root, path.file_name().unwrap().to_str().unwrap(), &changed)
        .unwrap();
    assert!(
        !recovered_target(&root, &item, "node check.cjs", &head)
            .unwrap()
            .unwrap()
            .success
    );
    std::fs::remove_file(path).unwrap();
    assert!(
        recovered_target(&root, &item, "node check.cjs", &head)
            .unwrap()
            .is_none()
    );
    std::fs::remove_dir_all(root).unwrap();
}
