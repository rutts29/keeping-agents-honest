//! One user-authorized legacy class migration; original records are immutable.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

pub fn eligible(wake: &Wake) -> bool {
    if wake.purpose != Purpose::Checkpoint
        || wake.trigger["review_class"] == "commit"
        || wake.trigger.get("verification_followup").is_some()
    {
        return false;
    }
    wake.trigger["events"].as_array().is_some_and(|events| {
        !events.is_empty()
            && events.iter().all(|event| match event["kind"].as_str() {
                Some("native_head" | "backlog_latest_commit") => true,
                Some("native_phase_claim") => {
                    event["judgment"]["backend"] == "jev"
                        && event["judgment"]["answers"]["phase_claim"]["noul"]
                            .as_f64()
                            .is_some_and(|p| {
                                p > kah_observer::intake::IGNORE_PROBABILITY && p < 0.8
                            })
                }
                _ => false,
            })
    })
}
pub fn convert(original: &Wake, authorization: &str) -> Result<Wake> {
    if authorization.is_empty() || !eligible(original) {
        return Err("REVIEW_LIGHT_MIGRATION_CLASS stage=queue_migration".into());
    }
    let mut converted = original.clone();
    let heads = original.trigger["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| e["head"].as_str())
        .filter(|h| valid_head(h))
        .collect::<BTreeSet<_>>();
    let head = original.trigger["head"]
        .as_str()
        .filter(|h| valid_head(h))
        .or_else(|| (heads.len() == 1).then(|| *heads.first().unwrap()));
    converted.trigger["head"] = json!(head);
    converted.trigger["review_class"] = "commit".into();
    converted.trigger["packet_scope"] = "diff-v1".into();
    converted.trigger["review_protocol"] = "commit-v1".into();
    converted
        .trigger
        .as_object_mut()
        .unwrap()
        .remove("reconsideration");
    converted.trigger["legacy_light_migration"] = json!({"authorization":authorization,
        "original_trigger":original.trigger,"historical_head_known":head.is_some()});
    Ok(converted)
}
fn valid_head(head: &str) -> bool {
    matches!(head.len(), 40 | 64) && head.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn run(worker: &str, authorization: &str) -> Result<Value> {
    if authorization.is_empty() {
        return Err("REVIEW_LIGHT_MIGRATION_AUTH stage=queue_migration".into());
    }
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    if Store::read(worker)?.is_none_or(|s| s.worker.session != worker) {
        return Err("REVIEW_MIGRATION_WORKER stage=queue_migration".into());
    }
    let id = format!("light-{:x}", Sha256::digest(authorization));
    let directory = root.join("queue-migrations");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let record = directory.join(format!("{id}.json"));
    let mut manifest = if let Some(saved) = super::super::state::read::<Value>(&record)? {
        saved
    } else {
        plan(&root, &id, authorization, worker)?
    };
    save(&directory, &record, &manifest)?;
    if manifest["state"] != "complete" {
        finish(&root, &manifest)?;
        manifest["state"] = "complete".into();
        manifest["queue_after"] = json!(super::entries_at(&root)?.len());
        save(&directory, &record, &manifest)?;
    }
    Ok(manifest)
}
fn plan(root: &Path, id: &str, authorization: &str, worker: &str) -> Result<Value> {
    let jobs = super::entries_at(root)?;
    metadata::stamps(root, &jobs)?;
    let mut rows = vec![];
    for (path, wake) in &jobs {
        if !eligible(wake) {
            continue;
        }
        let original = path.file_name().unwrap().to_str().unwrap();
        let replacement = format!("{id}-{original}");
        rows.push(json!({"original":original,"replacement":replacement,"digest":format!("{:x}",Sha256::digest(fs::read(path)?)),
            "disposition":"covered_by","converted":convert(wake,authorization)?}));
    }
    Ok(
        json!({"state":"planned","authorization":authorization,"queue_before":jobs.len(),"rows":rows,
        "pending_before":Store::read(worker)?.and_then(|s|s.pending),
        "at_ms":kah_observer::session::now_ms()}),
    )
}
fn finish(root: &Path, manifest: &Value) -> Result<()> {
    let archive = root.join("queue-retired");
    kah_core::run_store::ensure_private_directory(&archive)?;
    for row in manifest["rows"]
        .as_array()
        .ok_or("REVIEW_LIGHT_MIGRATION_RECORD stage=queue_migration")?
    {
        let old = row["original"].as_str().unwrap();
        let new = row["replacement"].as_str().unwrap();
        let source = root.join("queued").join(old);
        let retired = archive.join(old);
        let present = if retired.exists() { &retired } else { &source };
        if row["digest"] != format!("{:x}", Sha256::digest(fs::read(present)?)) {
            return Err("REVIEW_LIGHT_MIGRATION_SOURCE_CHANGED stage=queue_migration".into());
        }
        if !retired.exists() {
            fs::rename(&source, &retired)?;
        }
        let wake: Wake = serde_json::from_value(row["converted"].clone())?;
        let target = root.join("queued").join(new);
        if !target.exists() && !root.join("queue-retired").join(new).exists() {
            super::super::source_store::save(&root.join("queued"), new, &row["converted"])?;
        }
        ordering::renamed(root, old, new)?;
        metadata::renamed(root, old, new)?;
        metadata::reclassified(root, new, &wake)?;
    }
    collapse(root)?;
    fs::File::open(&archive)?.sync_all()?;
    fs::File::open(root.join("queued"))?.sync_all()?;
    Ok(())
}
fn collapse(root: &Path) -> Result<()> {
    let state = super::super::state::read::<super::super::state::State>(&root.join("state.json"))?
        .ok_or("REVIEW_STATE_MISSING stage=queue_migration")?;
    let worker = &state.worker.session;
    let active = state.pending.as_ref().and_then(|p| p.head.as_deref());
    let jobs = super::entries_at(root)?;
    for (path, wake) in jobs {
        if wake.trigger["review_class"] == "commit"
            && wake.trigger["head"].as_str().is_some()
            && wake.trigger["head"].as_str() != active
            && path.exists()
        {
            let replacement = format!("migration-collapse-{}.json", crate::relay_store::nonce());
            // Existing native ancestry logic preserves divergence and transfers
            // every claim/authority record. It also retains this original wake.
            let incoming = Wake {
                purpose: Purpose::Checkpoint,
                turn: wake.turn,
                trigger: json!({"review_class":"commit","head":wake.trigger["head"],"packet_scope":"diff-v1","review_protocol":"commit-v1"}),
            };
            supersede::publish(worker, root, &replacement, incoming)?;
        }
    }
    Ok(())
}
fn save(directory: &Path, record: &Path, value: &Value) -> Result<()> {
    super::super::source_store::save(
        directory,
        record.file_name().unwrap().to_str().unwrap(),
        value,
    )
}
pub fn cli(worker: &str, authorization: &str) -> Result<()> {
    let manifest = run(worker, authorization)?;
    println!(
        "{}",
        json!({"worker":worker,"authorization":authorization,
        "before":manifest["queue_before"],"after":manifest["queue_after"],
        "converted":manifest["rows"].as_array().map(Vec::len),
        "private_manifest":Store::directory(worker)?.join("queue-migrations"),"state":manifest["state"]})
    );
    Ok(())
}
