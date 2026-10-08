//! Queue control scans never resolve queued evidence bodies.
use super::*;
use crate::observer_review::source_store::{Select, read_selected};
pub fn at(root: &std::path::Path) -> Result<Vec<(PathBuf, Wake)>> {
    supersede::recover(root)?;
    let directory = root.join("queued");
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    let order = ordering::record(root, &paths)?;
    paths.sort_by_key(|p| {
        order
            .get(p.file_name().unwrap().to_str().unwrap())
            .copied()
            .unwrap_or(u64::MAX)
    });
    let selection = fields();
    let mut jobs = vec![];
    for path in paths {
        jobs.push((
            path.clone(),
            serde_json::from_value(read_selected(&path, &selection)?)?,
        ));
    }
    let stamps = metadata::stamps(root, &jobs)?;
    lanes::sort(
        &mut jobs,
        &stamps,
        metadata::last(root)?,
        kah_observer::session::now_ms(),
    )?;
    Ok(jobs)
}
fn fields() -> Select {
    let event = Select::fields(&[("kind", Select::All), ("head", Select::All)]);
    let trigger = Select::fields(&[
        ("review_class", Select::All),
        ("head", Select::All),
        ("events", Select::Each(Box::new(event))),
        ("captured_snapshot", Select::Raw),
        ("uncertain_user_input", Select::All),
        (
            "verification_followup",
            Select::fields(&[("stage", Select::All)]),
        ),
        ("preparation", Select::fields(&[("purpose", Select::All)])),
    ]);
    Select::fields(&[
        ("purpose", Select::All),
        ("turn", Select::All),
        ("trigger", trigger),
    ])
}
pub fn selected(worker: &str, jobs: &[(PathBuf, Wake)]) -> Result<Option<(Wake, Vec<PathBuf>)>> {
    let Some((header, paths)) = super::batch(jobs) else {
        return Ok(None);
    };
    if header.purpose == Purpose::Preparation && header.trigger.get("captured_snapshot").is_none() {
        return crate::observer_review::preparation::batch::packet_refs::group(
            worker, &header, &paths,
        )
        .map(Some);
    }
    let original = paths
        .iter()
        .map(|path| {
            Ok((
                path.clone(),
                crate::observer_review::source_store::read::<Wake>(path)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(super::batch(&original))
}
