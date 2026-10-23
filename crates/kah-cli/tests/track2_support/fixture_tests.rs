use super::*;

#[test]
fn a_private_parent_above_the_fixture_base_needs_sandbox_traversal() {
    // A mode-0700 directory above KAH_TEST_FIXTURE_BASE blocks the sandbox
    // account as much as the fixture directories below it.
    let operator = kah_sandbox::current_uid().unwrap();
    let outer = std::env::temp_dir().join(format!(
        "kah-fixture-ancestors-{}",
        kah_cli::relay_store::nonce()
    ));
    let private = outer.join("private");
    let root = private.join("fixtures/track2/run");
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&outer, fs::Permissions::from_mode(0o755)).unwrap();
    for path in root.ancestors().take_while(|p| p.starts_with(&private)) {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let outer = outer.canonicalize().unwrap();
    let found = private_ancestors(&root.canonicalize().unwrap(), operator);
    for path in ["private", "private/fixtures", "private/fixtures/track2"] {
        assert!(
            found.contains(&outer.join(path)),
            "{path} missing: {found:?}"
        );
    }
    assert!(found.contains(&outer.join("private/fixtures/track2/run")));
    assert!(
        !found.contains(&outer),
        "a traversable parent needs no grant"
    );
    assert!(
        found
            .iter()
            .all(|p| fs::metadata(p).unwrap().uid() == operator),
        "only the operator's own directories are changed"
    );
    fs::remove_dir_all(outer).unwrap();
}
