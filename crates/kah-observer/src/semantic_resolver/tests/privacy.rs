use super::*;

#[cfg(unix)]
#[test]
fn temporary_compactor_directory_and_files_are_private_at_creation() {
    use std::os::unix::fs::PermissionsExt;

    let files = TemporaryCallFiles::create().expect("create temporary compactor files");

    assert_eq!(
        fs::symlink_metadata(&files.directory)
            .expect("temporary directory metadata")
            .permissions()
            .mode()
            & 0o077,
        0
    );
    for path in [&files.schema, &files.final_message] {
        let metadata = fs::symlink_metadata(path).expect("temporary file metadata");
        assert!(metadata.is_file());
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(metadata.permissions().mode() & 0o077, 0);
    }
}
