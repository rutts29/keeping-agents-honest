use kah_cli::observer_review::{hook, state::Store};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

/// A kah whose own executable was replaced (/proc/self/exe names a deleted
/// inode) still starts detached jobs with the installed kah. A disposable home
/// without an installation runs this kah. A caller that is not kah, such as a
/// library test runner starts no job.
#[test]
fn detached_jobs_use_installed_kah_instead_of_the_callers_executable() {
    let root = std::env::temp_dir().join(format!("job-program-{}", std::process::id()));
    let (home, bare, bin) = (root.join("home"), root.join("bare"), root.join("bin"));
    for directory in [home.join(".local/bin"), bare.clone(), bin.clone()] {
        fs::create_dir_all(directory).unwrap();
    }
    let installed = home.join(".local/bin/kah");
    let current = bin.join("kah");
    for program in [&installed, &current] {
        fs::write(program, "#!/bin/sh\n").unwrap();
        fs::set_permissions(program, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let select = |current: &Path, home: &Path| hook::job_program(current, home).unwrap();
    assert_eq!(select(&current, &home), Some(installed.clone()));
    assert_eq!(select(&bin.join("kah (deleted)"), &home), Some(installed));
    assert_eq!(select(&current, &bare), Some(current.clone()));
    assert!(hook::job_program(&bin.join("kah (deleted)"), &bare).is_err());
    assert_eq!(select(&std::env::current_exe().unwrap(), &home), None);
    fs::remove_dir_all(root).unwrap();
}

/// A test runner records the spawn in the job log and starts nothing.
#[test]
fn a_test_runner_records_a_detached_job() {
    let worker = format!("job-program-record-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let root = store.directory.clone();
    drop(store);
    hook::spawn("__review-consume", &worker).unwrap();
    let output = fs::read_to_string(root.join("job-__review-consume.log")).unwrap();
    assert!(
        output.contains("\"recorded_spawn\":\"__review-consume\""),
        "{output}"
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
