use kah_run::access::{read_file, share_tree, traverse};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
};

#[test]
fn sandbox_access_inherits_operator_rights_without_following_links() {
    let root = std::env::temp_dir().join(format!("kah-run-access-{}", std::process::id()));
    fs::create_dir_all(root.join("tree/nested")).unwrap();
    fs::write(root.join("private"), "private").unwrap();
    fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("tree/nested/file"), "project").unwrap();
    symlink(root.join("private"), root.join("tree/link")).unwrap();
    share_tree(&root.join("tree"), 42420, true).unwrap();
    let acl = Command::new("getfacl")
        .args(["-cpn"])
        .arg(root.join("tree/nested"))
        .output()
        .unwrap();
    assert!(acl.status.success());
    let acl = String::from_utf8(acl.stdout).unwrap();
    assert!(acl.contains("user:42420:rwx"), "{acl}");
    assert!(acl.contains("default:user:1000:rwx"), "{acl}");
    assert_eq!(
        fs::metadata(root.join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(share_tree(&root.join("tree/link"), 42420, true).is_err());
    fs::remove_dir_all(root).unwrap();
}

fn acl(path: &Path) -> String {
    let out = Command::new("getfacl")
        .args(["-cpn"])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

fn setfacl(path: &Path, entries: &str) {
    let status = Command::new("setfacl")
        .args(["-m", entries])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

// Effective rights are the entry's bits ANDed with the mask.
fn effective(acl: &str, entry: &str) -> String {
    let field = |prefix: &str| {
        acl.lines()
            .find_map(|l| l.strip_prefix(prefix))
            .and_then(|r| r.split_whitespace().next())
            .unwrap()
            .to_string()
    };
    let (entry, mask) = (field(entry), field("mask::"));
    entry
        .chars()
        .zip(mask.chars())
        .map(|(e, m)| if e != '-' && m != '-' { e } else { '-' })
        .collect()
}

#[test]
fn traverse_keeps_a_restricted_mask_from_widening_other_named_users() {
    let root = std::env::temp_dir().join(format!("kah-run-mask-{}", std::process::id()));
    let dir = root.join("restricted");
    fs::create_dir_all(&dir).unwrap();
    setfacl(&dir, "u:65533:rwx,m::r--");
    assert_eq!(effective(&acl(&dir), "user:65533:"), "r--");
    traverse(&dir, 42420).unwrap();
    let after = acl(&dir);
    assert!(after.contains("user:42420:--x"), "{after}");
    assert!(after.contains("mask::r-x"), "{after}");
    assert_eq!(effective(&after, "user:42420:"), "--x");
    // Only the needed bit joins the prior mask; the write bit stays hidden.
    assert_eq!(effective(&after, "user:65533:"), "r-x", "{after}");
    traverse(&dir, 42420).unwrap();
    assert_eq!(acl(&dir), after, "a repeat grant changes nothing");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_file_adds_only_read_to_the_prior_mask() {
    let root = std::env::temp_dir().join(format!("kah-run-mask-file-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("setup");
    fs::write(&file, "x").unwrap();
    setfacl(&file, "u:65533:rw-,m::---");
    read_file(&file, 42420).unwrap();
    let after = acl(&file);
    assert_eq!(effective(&after, "user:42420:"), "r--", "{after}");
    assert_eq!(effective(&after, "user:65533:"), "r--", "{after}");
    fs::remove_dir_all(root).unwrap();
}
