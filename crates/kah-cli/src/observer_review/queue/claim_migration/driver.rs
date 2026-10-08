use super::*;
use kah_observer::judge::routing;
use sha2::{Digest, Sha256};
use std::path::Path;
pub fn run(worker: &str, authorization: &str) -> Result<Value> {
    if authorization.is_empty() {
        return Err("REVIEW_CLAIM_MIGRATION_AUTH stage=queue_migration".into());
    }
    let root = Store::directory(worker)?;
    let _owner = Store::delivery_at(&root.join("claim-migration-owner"), false)?;
    let home = std::path::PathBuf::from(
        std::env::var_os("HOME").ok_or("REVIEW_HOME stage=queue_migration")?,
    );
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?
    .ok_or("REVIEW_MIGRATION_WORKER stage=queue_migration")?
    .binding;
    if binding.session != worker {
        return Err("REVIEW_MIGRATION_WORKER stage=queue_migration".into());
    }
    let (paths, recovered) = {
        let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
        let recovered = storage::recover(&root)?;
        let paths = super::super::headers::at(&root)?
            .into_iter()
            .filter(|(_, w)| eligible(w))
            .map(|(p, _)| p)
            .collect::<Vec<_>>();
        (paths, recovered)
    };
    let before = paths.len();
    let mut records = vec![];
    for path in paths {
        if let Some(record) = classify(&root, &path, authorization, &home, &binding)? {
            records.push(record);
        }
    }
    {
        let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
        storage::recover(&root)?;
        if before > 0 || recovered > 0 {
            collapse(worker, &root)?;
        }
    }
    Ok(
        json!({"worker":worker,"authorization":authorization,"legacy_claims":before,"records":records,
        "queue_after":super::super::header_entries(worker)?.len()}),
    )
}
fn classify(
    root: &Path,
    path: &Path,
    authorization: &str,
    home: &Path,
    binding: &kah_observer::session::Binding,
) -> Result<Option<Value>> {
    let (original, digest) = {
        let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
        if !path.exists() {
            return Ok(None);
        }
        (
            crate::observer_review::source_store::read::<Wake>(path)?,
            format!("{:x}", Sha256::digest(fs::read(path)?)),
        )
    };
    if !eligible(&original) {
        return Ok(None);
    }
    let name = path.file_name().unwrap().to_str().unwrap();
    let directory = root.join("claim-migrations");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let id = format!(
        "claim-v{}-{}.json",
        scope::VERSION,
        format_args!(
            "{:x}",
            Sha256::digest(format!("{authorization}:{name}:{digest}"))
        )
    );
    let journal = directory.join(&id);
    if !journal.exists() {
        let judgments = original.trigger["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                if e["source"]["native_assistant_text"]
                    .as_str()
                    .is_none_or(|s| s.trim().is_empty())
                {
                    return Err("REVIEW_CLAIM_SOURCE_MISSING stage=queue_migration".into());
                }
                routing::evaluate(
                    home,
                    binding.judge,
                    &binding.jev_model,
                    &e["source"],
                    &scope::questions(),
                )
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        let converted = reclassify(&original, &judgments)?;
        storage::save(
            &journal,
            &json!({"state":"classified","authorization":authorization,"original":name,"digest":digest,
            "replacement":id,"converted":converted,"judgments":judgments,"questions":scope::questions(),
            "disposition":if converted.is_some(){"covered_by_reclassification"}else{"verified_not_a_claim"}}),
        )?;
    }
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    storage::finish(root, &journal)?;
    Ok(Some(json!({"original":name,"journal":journal})))
}
fn collapse(worker: &str, root: &Path) -> Result<()> {
    let control = crate::observer_review::state::read_control(worker)?;
    let active = control
        .as_ref()
        .and_then(|s| s.pending.as_ref())
        .and_then(|p| p.head.as_deref());
    let jobs = super::super::headers::at(root)?;
    for (path, header) in jobs {
        if !path.exists()
            || !(header.trigger["review_class"] == "commit" || supersede::claim_only(&header))
            || supersede::head(&header).is_err()
            || supersede::head(&header).ok() == active
        {
            continue;
        }
        let original = crate::observer_review::source_store::read::<Wake>(&path)?;
        supersede::republish(
            worker,
            root,
            &path,
            &format!("claim-collapse-{}.json", crate::relay_store::nonce()),
            original,
        )?;
    }
    Ok(())
}
pub fn cli(worker: &str, authorization: &str) -> Result<()> {
    println!("{}", run(worker, authorization)?);
    Ok(())
}
