use std::{fs, path::Path};

/// Check every source file under `root`, skipping a build directory named by
/// `CARGO_TARGET_DIR` (relative values resolve against `root`).
pub fn check_tree(root: &Path) -> Result<(), String> {
    let target = std::env::var_os("CARGO_TARGET_DIR").map(|dir| root.join(dir));
    check_tree_excluding(root, target.as_deref())
}

/// Check every source file under `root` except the `excluded` subtree.
pub fn check_tree_excluding(root: &Path, excluded: Option<&Path>) -> Result<(), String> {
    let excluded = excluded.and_then(|path| fs::canonicalize(path).ok());
    visit(root, excluded.as_deref())
}

fn visit(path: &Path, excluded: Option<&Path>) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let kind = entry.file_type().map_err(io_error)?;
        if kind.is_dir() {
            if !skipped(&entry.file_name().to_string_lossy(), &path, excluded) {
                visit(&path, excluded)?;
            }
        } else if kind.is_file() {
            check_file(&path)?;
        }
    }
    Ok(())
}

fn skipped(name: &str, path: &Path, excluded: Option<&Path>) -> bool {
    ["target", ".git"].contains(&name)
        || excluded.is_some_and(|dir| fs::canonicalize(path).is_ok_and(|real| real == dir))
}

fn check_file(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(io_error)?;
    let source = String::from_utf8_lossy(&bytes);
    let limit = if is_shell(path, &source) {
        99
    } else if is_test(path) {
        600
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        400
    } else {
        return Ok(());
    };
    let lines = source.lines().count();
    if lines > limit {
        return Err(format!(
            "FILE_TOO_LARGE stage=size path={} lines={lines} limit={limit}",
            path.display()
        ));
    }
    Ok(())
}

fn is_shell(path: &Path, source: &str) -> bool {
    if path.extension().is_some_and(|ext| ext == "sh") {
        return true;
    }
    source
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("#!"))
        .is_some_and(|line| {
            line.split_whitespace().any(|word| {
                Path::new(word).file_name().is_some_and(|name| {
                    ["sh", "bash", "dash", "zsh", "ksh"]
                        .iter()
                        .any(|shell| name == *shell)
                })
            })
        })
}

fn is_test(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == "tests")
        || path.file_name().is_some_and(|name| name == "tests.rs")
}

fn io_error(error: std::io::Error) -> String {
    format!("CHECK_IO stage=size: {error}")
}
