//! Inputs and output roots for opt-in replay tests.
//!
//! Retained data is never shipped. `KAH_TEST_CORPUS_DIR` names a directory the
//! operator supplies; there is no default. Output goes below the fixture base.
#![allow(dead_code)]
use std::path::{Path, PathBuf};

/// `KAH_TEST_FIXTURE_BASE`, or a directory under the system temporary one.
pub fn fixture_base() -> PathBuf {
    std::env::var_os("KAH_TEST_FIXTURE_BASE")
        .map_or_else(|| std::env::temp_dir().join("kah-fixtures"), PathBuf::from)
}

/// The canonical corpus root; fails when it is not supplied.
pub fn root() -> PathBuf {
    let root = std::env::var_os("KAH_TEST_CORPUS_DIR")
        .expect("CORPUS_MISSING stage=input: set KAH_TEST_CORPUS_DIR to the retained corpus");
    Path::new(&root)
        .canonicalize()
        .expect("CORPUS_ROOT stage=input")
}

/// A corpus file by relative path; it may not escape the root.
pub fn path(relative: &str) -> PathBuf {
    let root = root();
    let path = root
        .join(relative)
        .canonicalize()
        .unwrap_or_else(|error| panic!("CORPUS_FILE stage=input: {relative}: {error}"));
    assert!(
        path.starts_with(&root),
        "CORPUS_ESCAPE stage=input: {relative}"
    );
    path
}

/// Resolve a path carried by a corpus record against the corpus root.
pub fn locator(carried: &str) -> PathBuf {
    path(carried)
}

/// A path derived from a corpus file; it must stay inside the corpus root.
pub fn within(derived: &Path) -> PathBuf {
    let root = root();
    let real = derived
        .canonicalize()
        .unwrap_or_else(|error| panic!("CORPUS_FILE stage=input: {}: {error}", derived.display()));
    assert!(
        real.starts_with(&root),
        "CORPUS_ESCAPE stage=input: {}",
        derived.display()
    );
    real
}

/// A new output below the fixture base. The base and the output's parent are
/// canonical, the output does not exist, and it does not overlap an input.
pub fn fresh_output(output: &Path, inputs: &[&Path]) -> PathBuf {
    let base = fixture_base();
    std::fs::create_dir_all(&base).expect("OUTPUT_BASE stage=output");
    let base = base.canonicalize().expect("OUTPUT_BASE stage=output");
    let parent = output
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .expect("OUTPUT_PARENT stage=output: the parent directory must exist");
    assert!(
        parent.starts_with(&base),
        "OUTPUT_SCOPE stage=output: {}",
        output.display()
    );
    let name = output.file_name().expect("OUTPUT_NAME stage=output");
    let output = parent.join(name);
    assert!(
        std::fs::symlink_metadata(&output).is_err(),
        "OUTPUT_EXISTS stage=output: {}",
        output.display()
    );
    for input in inputs {
        let input = input.canonicalize().expect("OUTPUT_INPUT stage=output");
        assert!(
            !output.starts_with(&input) && !input.starts_with(&output),
            "OUTPUT_OVERLAP stage=output: {}",
            output.display()
        );
    }
    output
}
