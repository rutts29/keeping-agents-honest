use kah_core::run_store::ensure_private_directory;
use std::{
    fs,
    sync::{Arc, Barrier},
};
#[test]
fn concurrent_private_parent_creation_succeeds_without_weakening_path_checks() {
    let root = std::env::temp_dir().join(format!("kah-private-create-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    for round in 0..24 {
        let path = root.join(format!("round-{round}/state/repositories"));
        let barrier = Arc::new(Barrier::new(8));
        std::thread::scope(|scope| {
            let mut jobs = vec![];
            for _ in 0..8 {
                let barrier = barrier.clone();
                let path = &path;
                jobs.push(scope.spawn(move || {
                    barrier.wait();
                    ensure_private_directory(path)
                }));
            }
            for job in jobs {
                assert!(job.join().unwrap().is_ok());
            }
        });
    }
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, root.join("link")).unwrap();
    assert!(ensure_private_directory(&root.join("link/child")).is_err());
    fs::File::create(root.join("file")).unwrap();
    assert!(ensure_private_directory(&root.join("file")).is_err());
    fs::remove_dir_all(root).unwrap();
}
