use super::{enrollment_path, redacted_session};
use std::path::Path;

#[test]
fn enrollment_paths_and_labels_do_not_expose_private_session_ids() {
    let session = "private/session";
    let path = enrollment_path(Path::new("/state"), session);
    assert!(!path.to_string_lossy().contains("private"));
    assert!(!redacted_session(session).contains("private"));
}
