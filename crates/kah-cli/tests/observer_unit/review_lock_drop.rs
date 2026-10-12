//! Deterministic REVIEW_BUSY after drop in the retained post-ACK regression.
use super::*;
use std::os::fd::AsRawFd;

#[test]
fn review_store_drop_releases_immediately_while_an_inherited_description_survives() {
    std::env::var_os("XDG_STATE_HOME").expect("isolated check-tests fixture required");
    let worker = format!("review-drop-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let directory = store.directory.clone();
    let info =
        fs::read_to_string(format!("/proc/self/fdinfo/{}", store._lock.as_raw_fd())).unwrap();
    let flags = info
        .lines()
        .find_map(|s| s.strip_prefix("flags:\t"))
        .unwrap();
    assert_ne!(
        u32::from_str_radix(flags.trim(), 8).unwrap() & 0o2000000,
        0,
        "O_CLOEXEC must be set"
    );
    let inherited = store._lock.try_clone().unwrap();
    assert!(Store::open(&worker).is_err());
    drop(store);
    let reopened = Store::open(&worker)
        .expect("Review Store Drop must release before inherited handles close");
    drop(reopened);
    drop(inherited);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
}
