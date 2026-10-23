use kah_run::access::{read_file, share_tree, share_tree_owned_by, traverse};
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
    share_tree(&root.join("tree"), 65532, true).unwrap();
    let acl = Command::new("getfacl")
        .args(["-cpn"])
        .arg(root.join("tree/nested"))
        .output()
        .unwrap();
    assert!(acl.status.success());
    let acl = String::from_utf8(acl.stdout).unwrap();
    assert!(acl.contains("user:65532:rwx"), "{acl}");
    let operator = kah_sandbox::current_uid().unwrap();
    assert!(
        acl.contains(&format!("default:user:{operator}:rwx")),
        "{acl}"
    );
    assert_eq!(
        fs::metadata(root.join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(share_tree(&root.join("tree/link"), 65532, true).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_directory_of_another_owner_never_blocks_the_share_walk() {
    // A sandboxed observer once left outbox/build-o48f owned by a mapped uid
    // with mode 0700. The operator could not read it, so find failed with
    // EACCES and review admission stopped.
    let root = std::env::temp_dir().join(format!("kah-run-foreign-{}", std::process::id()));
    let foreign = root.join("tree/build-o48f");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("build.py"), "x").unwrap();
    fs::set_permissions(&foreign, fs::Permissions::from_mode(0o000)).unwrap();
    // Without root a test cannot chown. This process owns every entry, so
    // name another operator: the directory then has another owner, and the
    // walk cannot read it, as in that failure.
    let operator = kah_sandbox::current_uid().unwrap() + 1;
    let result = share_tree_owned_by(&root.join("tree"), 65532, true, operator);
    fs::set_permissions(&foreign, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir_all(root).unwrap();
    result.unwrap();
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
    traverse(&dir, 65532).unwrap();
    let after = acl(&dir);
    assert!(after.contains("user:65532:--x"), "{after}");
    assert!(after.contains("mask::r-x"), "{after}");
    assert_eq!(effective(&after, "user:65532:"), "--x");
    // Only the needed bit joins the prior mask; the write bit stays hidden.
    assert_eq!(effective(&after, "user:65533:"), "r-x", "{after}");
    traverse(&dir, 65532).unwrap();
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
    read_file(&file, 65532).unwrap();
    let after = acl(&file);
    assert_eq!(effective(&after, "user:65532:"), "r--", "{after}");
    assert_eq!(effective(&after, "user:65533:"), "r--", "{after}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_files_that_vanished_under_the_root_are_tolerated() {
    use kah_run::access::vanished_only;
    let root = Path::new("/home/u/.claude/plugins");
    // 5h-f on 436807d: plugin sync removed this file during the walk.
    let vanished = "/usr/bin/find: /home/u/.claude/plugins/synced/419e/.last-complete-round: No such file or directory\n";
    assert!(vanished_only(vanished, root));
    assert!(vanished_only(
        "setfacl: ./.last-complete-round: No such file or directory\n",
        root
    ));
    assert!(vanished_only(
        "/usr/bin/find: ‘/home/u/.claude/plugins/x’: No such file or directory\n",
        root
    ));
    assert!(vanished_only(
        "/usr/bin/find: '/home/u/.claude/plugins/x': No such file or directory\n",
        root
    ));
    assert!(!vanished_only("", root));
    assert!(!vanished_only(
        "/usr/bin/find: /elsewhere/x: No such file or directory\n",
        root
    ));
    assert!(!vanished_only(
        "/usr/bin/find: /home/u/.claude/plugins/a: Permission denied\n",
        root
    ));
    assert!(!vanished_only(
        "setfacl: ./a: Operation not permitted\n",
        root
    ));
    // One real failure among vanished files still fails the share step.
    let mixed = format!("{vanished}setfacl: ./b: Operation not permitted\n");
    assert!(!vanished_only(&mixed, root));
}

#[test]
fn files_deleted_during_the_share_walk_do_not_fail_it() {
    let root = std::env::temp_dir().join(format!("kah-run-vanish-{}", std::process::id()));
    let tree = root.join("tree");
    for round in 0..5 {
        for i in 0..400 {
            let dir = tree.join(format!("d{}", i % 20));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(format!("f{round}-{i}")), "x").unwrap();
        }
        let deleting = tree.clone();
        let remover = std::thread::spawn(move || {
            for entry in walk(&deleting) {
                let _ = fs::remove_file(entry);
            }
        });
        share_tree(&tree, 65532, true).unwrap();
        remover.join().unwrap();
    }
    fs::remove_dir_all(root).unwrap();
}

fn walk(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = vec![];
    for dir in fs::read_dir(root).unwrap().flatten() {
        if let Ok(entries) = fs::read_dir(dir.path()) {
            files.extend(entries.flatten().map(|e| e.path()));
        }
    }
    files
}
