//! Derived versions beside History, committed under the existing run writer lock.
use super::*;
use crate::current_rules::{Input, RuleSet, Source, ValidatedSet};

#[derive(Serialize, Deserialize)]
struct Version {
    input: Input,
    set: RuleSet,
}
#[derive(Serialize, Deserialize)]
struct Head {
    digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuleRebuild {
    pub input: Input,
    pub previous: Option<RuleSet>,
    pub previous_digest: Option<String>,
    pub direct_observer_review: bool,
}
pub enum RuleSync {
    Ready(ValidatedSet),
    Rebuild(Box<RuleRebuild>),
}
impl RunStore {
    /// No inference occurs here. A rebuild goes to the existing native observer.
    pub fn sync_current_rules(
        &self,
        standing: Vec<Source>,
        observed_at: &str,
    ) -> Result<RuleSync, String> {
        let input = self.rule_input(standing, observed_at)?;
        let previous = self.accepted_current_rules()?;
        if let Some(set) = &previous
            && set.as_set().input_digest == input.digest()?
        {
            return Ok(RuleSync::Ready(set.clone()));
        }
        let request = RuleRebuild {
            input,
            previous_digest: previous.as_ref().map(ValidatedSet::digest).transpose()?,
            previous: previous.map(|v| v.as_set().clone()),
            direct_observer_review: true,
        };
        replace(&self.path, "current-rules-request.json", &request)?;
        Ok(RuleSync::Rebuild(Box::new(request)))
    }
    /// Caller supplies freshly read standing snapshots, never model-authored ones.
    pub fn accept_current_rules(
        &self,
        candidate: &RuleSet,
        standing: Vec<Source>,
        observed_at: &str,
    ) -> Result<ValidatedSet, String> {
        let input = self.rule_input(standing, observed_at)?;
        let previous = self.accepted_current_rules()?;
        let validated = candidate.validate(&input, previous.as_ref())?;
        let versions = self.path.join("current-rules");
        ensure_private_directory(&versions).map_err(rule_io)?;
        let digest = validated.digest()?;
        let path = version_path(&versions, &digest)?;
        let version = Version {
            input,
            set: candidate.clone(),
        };
        if let Some(old) = read::<Version>(&path)? {
            if old.set != version.set || old.input.digest()? != version.input.digest()? {
                return Err(
                    "RULE_SET_IMMUTABLE stage=current_rules: existing version changed".into(),
                );
            }
        } else {
            let bytes = serde_json::to_vec(&version).map_err(rule_io)?;
            write_new_private_file_bytes(&path, &bytes, "save rule-set version")
                .map_err(rule_io)?;
            sync_directory(&versions).map_err(rule_io)?;
        }
        replace(&self.path, "current-rules.json", &Head { digest })?;
        Ok(validated)
    }
    /// Replay provenance and every predecessor; a malformed stored set is never trusted.
    pub fn accepted_current_rules(&self) -> Result<Option<ValidatedSet>, String> {
        let Some(head) = read::<Head>(&self.path.join("current-rules.json"))? else {
            return Ok(None);
        };
        ensure_private_directory(&self.path.join("current-rules")).map_err(rule_io)?;
        let mut next = Some(head.digest);
        let mut seen = BTreeSet::new();
        let mut versions = Vec::new();
        while let Some(digest) = next {
            if !seen.insert(digest.clone()) {
                return Err("RULE_SET_CYCLE stage=current_rules".into());
            }
            let path = version_path(&self.path.join("current-rules"), &digest)?;
            let version =
                read::<Version>(&path)?.ok_or("RULE_SET_VERSION_MISSING stage=current_rules")?;
            if crate::current_rules::digest(&version.set)? != digest {
                return Err("RULE_SET_DIGEST stage=current_rules: changed archived version".into());
            }
            next = version.set.previous_digest.clone();
            versions.push(version);
        }
        let mut previous = None;
        for version in versions.into_iter().rev() {
            previous = Some(version.set.validate(&version.input, previous.as_ref())?);
        }
        Ok(previous)
    }
    fn rule_input(&self, standing: Vec<Source>, observed_at: &str) -> Result<Input, String> {
        let recovered = self.recover().map_err(rule_io)?;
        Input::from_history(&recovered.history, standing, observed_at)
    }
}
fn version_path(directory: &Path, digest: &str) -> Result<PathBuf, String> {
    let hash = digest
        .strip_prefix("sha256:")
        .ok_or("RULE_SET_DIGEST stage=current_rules")?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("RULE_SET_DIGEST stage=current_rules".into());
    }
    Ok(directory.join(format!("{hash}.json")))
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    ensure_private_regular_or_missing(path).map_err(rule_io)?;
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(rule_io),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(rule_io(e)),
    }
}
fn replace(directory: &Path, name: &str, value: &impl Serialize) -> Result<(), String> {
    let path = directory.join(name);
    ensure_private_regular_or_missing(&path).map_err(rule_io)?;
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
    let bytes = serde_json::to_vec(value).map_err(rule_io)?;
    write_new_private_file_bytes(&temporary, &bytes, "save current rules").map_err(rule_io)?;
    fs::rename(&temporary, &path).map_err(rule_io)?;
    sync_directory(directory).map_err(rule_io)
}
fn rule_io(error: impl fmt::Display) -> String {
    format!("RULE_SET_STORE stage=current_rules: {error}")
}
