use std::{fs, path::Path};

pub fn check_tree(root: &Path) -> Result<(), String> {
    visit(root)
}

fn visit(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        let kind = entry.file_type().map_err(io_error)?;
        if kind.is_dir() {
            if !["target", ".git"].contains(&entry.file_name().to_string_lossy().as_ref()) {
                visit(&path)?;
            }
        } else if kind.is_file()
            && (is_test(&path) || path.extension().is_some_and(|ext| ext == "rs"))
        {
            check_file(&path)?;
        }
    }
    Ok(())
}

fn check_file(path: &Path) -> Result<(), String> {
    let source = fs::read_to_string(path).map_err(io_error)?;
    let limit = if is_test(path) { 600 } else { 400 };
    let lines = source.lines().count();
    if lines > limit {
        return Err(format!(
            "FILE_TOO_LARGE stage=size path={} lines={lines} limit={limit}",
            path.display()
        ));
    }
    Ok(())
}

fn is_test(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == "tests")
        || path.file_name().is_some_and(|name| name == "tests.rs")
}

fn io_error(error: std::io::Error) -> String {
    format!("CHECK_IO stage=size: {error}")
}
