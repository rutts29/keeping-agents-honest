//! Preserve private, scrubbed failure evidence before native fixture purge.
use kah_cli::observer_review::state::{State, Store as Reviews};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn archive(home: &Path, state: &State) -> kah_cli::Result<()> {
    assert!(
        state
            .worker
            .socket
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .contains("kah-gate-")
    );
    let root = home
        .join(".local/share/kah-eval/replay-traces")
        .join(format!(
            "{}-{}",
            state.worker.session,
            kah_observer::session::now_ms()
        ));
    kah_core::run_store::ensure_private_directory(&root)?;
    let review = Reviews::directory(&state.worker.session)?;
    retain(&review, &root)?;
    retain(
        &state.observer.workspace.join("outbox"),
        &root.join("outbox"),
    )?;
    if let Some(path) = &state.observer.transcript {
        let mut rows = json!(
            fs::read_to_string(path)?
                .lines()
                .map(|line| serde_json::from_str::<Value>(line)
                    .unwrap_or_else(|_| json!({"raw":line})))
                .collect::<Vec<_>>()
        );
        crate::privacy::scrub(&mut rows);
        kah_cli::observer_review::source_store::save(&root, "native-transcript.json", &rows)?;
    }
    eprintln!("private review trace: {}", root.display());
    Ok(())
}
fn retain(source: &Path, destination: &Path) -> kah_cli::Result<()> {
    if !source.exists() {
        return Ok(());
    }
    kah_core::run_store::ensure_private_directory(destination)?;
    let entries = match fs::read_dir(source) {
        // The observer sandbox can create mode-0700 directories owned by
        // another UID. Record the unreadable directory without failing the trace.
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!(
                "private review trace: skipped {}: {error}",
                source.display()
            );
            return kah_cli::unattended_report::save(
                destination,
                "unreadable.json",
                &json!({"skipped":source,"error":error.to_string()}),
            );
        }
        entries => entries?,
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            retain(&path, &target)?;
        } else if metadata.is_file() && source.file_name().is_some_and(|s| s == "sources") {
            let digest = entry.file_name().to_string_lossy().into_owned();
            if digest.len() == 64 || digest.starts_with("block-") {
                kah_cli::observer_review::source_store::verify_blob(&path)?;
                fs::hard_link(&path, &target)?;
            }
        } else if metadata.is_file()
            && matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("json" | "jsonl" | "log" | "done")
            )
        {
            let text = fs::read_to_string(&path)?;
            let mut value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"rows":text.lines().map(|line|serde_json::from_str::<Value>(line).unwrap_or_else(|_|json!({"raw":line}))).collect::<Vec<_>>()}));
            crate::privacy::scrub(&mut value);
            kah_cli::unattended_report::save(
                destination,
                entry.file_name().to_str().unwrap(),
                &value,
            )?;
        }
    }
    Ok(())
}

#[test]
fn retain_notes_an_unreadable_directory_and_goes_on() {
    // Requires an unprivileged runner: mode 000 must deny directory reads.
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("kah-review-trace-{}", std::process::id()));
    let foreign = root.join("outbox/foreign-build");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(root.join("outbox/report.log"), "kept").unwrap();
    fs::set_permissions(&foreign, fs::Permissions::from_mode(0o000)).unwrap();
    let result = retain(&root.join("outbox"), &root.join("trace"));
    fs::set_permissions(&foreign, fs::Permissions::from_mode(0o700)).unwrap();
    result.unwrap();
    assert!(root.join("trace/report.log").is_file());
    assert!(root.join("trace/foreign-build/unreadable.json").is_file());
    fs::remove_dir_all(root).unwrap();
}
