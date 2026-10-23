use kah_core::unattended::{Limits, RunControl};
use kah_run::{
    commands,
    store::{Record, Store},
};
use std::{fs, path::PathBuf};

fn record() -> Record {
    Record {
        native: None,
        project: PathBuf::from("/fixture/project"),
        domain: "mine".into(),
        worktree: PathBuf::from("/fixture/run"),
        model: None,
        effort: None,
        control: RunControl::new("Implement sum", Limits::default()).unwrap(),
    }
}

#[test]
fn one_unreadable_run_does_not_hide_the_healthy_ones() {
    let root = std::env::temp_dir().join(format!("kah-run-list-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    drop(Store::create(&root, "run-a", record()).unwrap());
    drop(Store::create(&root, "run-c", record()).unwrap());
    // A creator that stopped before its first save, and a damaged state.
    fs::create_dir(root.join("run-b-half")).unwrap();
    fs::create_dir(root.join("run-d-bad")).unwrap();
    fs::write(root.join("run-d-bad/state.json"), "{not json").unwrap();
    fs::write(root.join("notes.txt"), "not a run").unwrap();

    let (runs, problems) = commands::list_with_problems(&root).expect("the base is readable");
    let ids: Vec<&str> = runs.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["run-a", "run-c"]);
    let skipped: Vec<&str> = problems.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(skipped, ["run-b-half", "run-d-bad"]);
    assert!(problems.iter().all(|(_, reason)| reason.contains("RUN_")));
    assert_eq!(commands::list(&root).unwrap().len(), 2);

    // Reading one run by id stays strict.
    assert!(commands::view(&root, "run-b-half").is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unreadable_base_directory_is_still_an_error() {
    let file = std::env::temp_dir().join(format!("kah-run-list-file-{}", std::process::id()));
    fs::write(&file, "not a directory").unwrap();
    assert!(commands::list_with_problems(&file).is_err());
    assert!(commands::list(&file).is_err());
    fs::remove_file(file).unwrap();
}
