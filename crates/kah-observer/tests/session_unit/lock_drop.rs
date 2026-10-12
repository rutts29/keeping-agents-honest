//! The retained parallel startup failure: an inherited open description outlives Store.
use super::*;
use std::os::fd::AsRawFd;

#[test]
fn dropping_store_releases_its_lock_even_while_an_inherited_description_survives() {
    let root = std::env::temp_dir().join(format!(
        "kah-observer-lock-drop-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&root, "startup-lock-replay", Duration::ZERO).unwrap();
    let info =
        fs::read_to_string(format!("/proc/self/fdinfo/{}", store._lock.as_raw_fd())).unwrap();
    let flags = info
        .lines()
        .find_map(|line| line.strip_prefix("flags:\t"))
        .unwrap();
    let flags = u32::from_str_radix(flags.trim(), 8).unwrap();
    assert_ne!(
        flags & 0o2000000,
        0,
        "Linux O_CLOEXEC must be set on the real lock descriptor"
    );
    // A forked child has the same open description until exec. CLOEXEC is
    // necessary, but closing the parent's descriptor alone cannot release it.
    let inherited = store._lock.try_clone().unwrap();
    assert!(Store::open(&root, "startup-lock-replay", Duration::ZERO).is_err());
    drop(store);
    let reopened = Store::open(&root, "startup-lock-replay", Duration::ZERO)
        .expect("Store Drop must release immediately despite the inherited descriptor");
    drop(reopened);
    drop(inherited);
    fs::remove_dir_all(root).unwrap();
}
