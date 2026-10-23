use kah_run::access::{read_file, share_tree, share_tree_owned_by, traverse};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
};

fn fixture_uids() -> (u32, u32) {
    let owner = kah_sandbox::current_uid().unwrap();
    let mut ids = (42420..42424).filter(|uid| *uid != owner);
    (ids.next().unwrap(), ids.next().unwrap())
}

#[test]
fn sandbox_access_inherits_operator_rights_without_following_links() {
    let sandbox = fixture_uids().0;
    let root = std::env::temp_dir().join(format!("kah-run-access-{}", std::process::id()));
    fs::create_dir_all(root.join("tree/nested")).unwrap();
    fs::write(root.join("private"), "private").unwrap();
    fs::set_permissions(root.join("private"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("tree/nested/file"), "project").unwrap();
    symlink(root.join("private"), root.join("tree/link")).unwrap();
    share_tree(&root.join("tree"), sandbox, true).unwrap();
    let acl = Command::new("getfacl")
        .args(["-cpn"])
        .arg(root.join("tree/nested"))
        .output()
        .unwrap();
    assert!(acl.status.success());
    let acl = String::from_utf8(acl.stdout).unwrap();
    assert!(acl.contains(&format!("user:{sandbox}:rwx")), "{acl}");
    assert!(
        acl.contains(&format!(
            "default:user:{}:rwx",
            kah_sandbox::current_uid().unwrap()
        )),
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
    assert!(share_tree(&root.join("tree/link"), sandbox, true).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_directory_of_another_owner_never_blocks_the_share_walk() {
    // A sandbox-created directory was mode 0700 and owned by another UID.
    // The operator could not read it, so find returned EACCES and blocked admission.
    let root = std::env::temp_dir().join(format!("kah-run-foreign-{}", std::process::id()));
    let foreign = root.join("tree/foreign-build");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("build.py"), "x").unwrap();
    fs::set_permissions(&foreign, fs::Permissions::from_mode(0o000)).unwrap();
    // Without root a test cannot chown. This process owns every entry, so
    // name another operator: the directory then has another owner, and the
    // walk cannot read it.
    let (sandbox, operator) = fixture_uids();
    let result = share_tree_owned_by(&root.join("tree"), sandbox, true, operator);
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
    let (sandbox, other) = fixture_uids();
    let root = std::env::temp_dir().join(format!("kah-run-mask-{}", std::process::id()));
    let dir = root.join("restricted");
    fs::create_dir_all(&dir).unwrap();
    setfacl(&dir, &format!("u:{other}:rwx,m::r--"));
    assert_eq!(effective(&acl(&dir), &format!("user:{other}:")), "r--");
    traverse(&dir, sandbox).unwrap();
    let after = acl(&dir);
    assert!(after.contains(&format!("user:{sandbox}:--x")), "{after}");
    assert!(after.contains("mask::r-x"), "{after}");
    assert_eq!(effective(&after, &format!("user:{sandbox}:")), "--x");
    // Only the needed bit joins the prior mask; the write bit stays hidden.
    assert_eq!(
        effective(&after, &format!("user:{other}:")),
        "r-x",
        "{after}"
    );
    traverse(&dir, sandbox).unwrap();
    assert_eq!(acl(&dir), after, "a repeat grant changes nothing");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_file_adds_only_read_to_the_prior_mask() {
    let (sandbox, other) = fixture_uids();
    let root = std::env::temp_dir().join(format!("kah-run-mask-file-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("setup");
    fs::write(&file, "x").unwrap();
    setfacl(&file, &format!("u:{other}:rw-,m::---"));
    read_file(&file, sandbox).unwrap();
    let after = acl(&file);
    assert_eq!(
        effective(&after, &format!("user:{sandbox}:")),
        "r--",
        "{after}"
    );
    assert_eq!(
        effective(&after, &format!("user:{other}:")),
        "r--",
        "{after}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_files_that_vanished_under_the_root_are_tolerated() {
    use kah_run::access::vanished_only;
    let root = Path::new("/home/u/.claude/plugins");
    // Plugin synchronization removed a file during the walk.
    let vanished = "/usr/bin/find: /home/u/.claude/plugins/synced/example-cache/.fixture-sync-marker: No such file or directory\n";
    assert!(vanished_only(vanished, root));
    assert!(vanished_only(
        "setfacl: ./.fixture-sync-marker: No such file or directory\n",
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
    let sandbox = fixture_uids().0;
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
        share_tree(&tree, sandbox, true).unwrap();
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
