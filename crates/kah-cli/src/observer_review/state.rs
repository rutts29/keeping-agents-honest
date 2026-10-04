use crate::{Result, relay_store::Endpoint};
use kah_core::run_store::{RunStore, ensure_private_directory};
use kah_observer::{review_evidence::Document, session::Store as Sessions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Preparation,
    Drift,
    Steering,
    Checkpoint,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub id: String,
    pub worker: String,
    pub purpose: Purpose,
    pub turn: String,
    pub head: Option<String>,
    pub intent: Value,
    pub trigger: Value,
    pub documents: Vec<Document>,
    pub created_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observer {
    pub session: Option<String>,
    pub pane: Option<String>,
    pub name: String,
    pub workspace: PathBuf,
    pub model: String,
    pub effort: String,
    pub transcript: Option<PathBuf>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub process_stamp: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub id: String,
    pub purpose: Purpose,
    pub turn: String,
    pub head: Option<String>,
    pub created_ms: u64,
    pub native_tokens_before: Option<u64>,
}
impl From<&Request> for Pending {
    fn from(r: &Request) -> Self {
        Self {
            id: r.id.clone(),
            purpose: r.purpose.clone(),
            turn: r.turn.clone(),
            head: r.head.clone(),
            created_ms: r.created_ms,
            native_tokens_before: None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub worker: Endpoint,
    pub observer: Observer,
    pub pending: Option<Pending>,
    pub latest: Option<Value>,
    pub last_verified_turn: Option<String>,
    pub checkpoint: Option<Value>,
    pub deferrals: Vec<Value>,
    pub wakes: u64,
    pub native_tokens: u64,
    pub stopped: bool,
}
impl State {
    pub fn new(worker: Endpoint, home: &Path) -> Result<Self> {
        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(worker.session.as_bytes()));
        let saved = Sessions::read(&RunStore::default_state_root()?, &worker.session)?
            .ok_or("REVIEW_BINDING stage=observer_config")?;
        let model = if worker.provider == "claude" && saved.binding.model.starts_with("gpt-") {
            "claude-sonnet-5-5"
        } else {
            &saved.binding.model
        };
        let choice =
            kah_observer::compaction::CompactorConfig::new(model, Some(&saved.binding.effort))?;
        let observer = Observer {
            session: None,
            pane: None,
            name: format!("kah-observer-{}", &hash[..16]),
            workspace: home.join(".cache/kah-observers").join(&hash[..24]),
            model: choice.model().into(),
            effort: choice.reasoning().into(),
            transcript: None,
            pid: None,
            process_stamp: None,
        };
        Ok(Self {
            worker,
            observer,
            pending: None,
            latest: None,
            last_verified_turn: None,
            checkpoint: None,
            deferrals: vec![],
            wakes: 0,
            native_tokens: 0,
            stopped: false,
        })
    }
}
pub struct Store {
    pub directory: PathBuf,
    _lock: File,
}
impl Store {
    pub fn directory(worker: &str) -> Result<PathBuf> {
        Ok(Sessions::directory(&RunStore::default_state_root()?, worker).join("review"))
    }
    pub fn open(worker: &str) -> Result<Self> {
        Self::lock(worker, false)
    }
    /// Detached jobs wait for the owner to release its OS lock. No review
    /// deadline: a live judge or native turn may legitimately take minutes.
    pub fn open_wait(worker: &str) -> Result<Self> {
        Self::lock(worker, true)
    }
    pub fn open_delivery(worker: &str) -> Result<Self> {
        Self::delivery_at(&Self::directory(worker)?, false)
    }
    pub fn delivery_at(directory: &Path, wait: bool) -> Result<Self> {
        Self::lock_directory(directory.to_path_buf(), wait, "delivery-lock")
    }
    fn lock(worker: &str, wait: bool) -> Result<Self> {
        let directory = Self::directory(worker)?;
        Self::lock_directory(directory, wait, "lock")
    }
    fn lock_directory(directory: PathBuf, wait: bool, name: &str) -> Result<Self> {
        ensure_private_directory(&directory)?;
        ensure_private_directory(&directory.join("sources"))?;
        let path = directory.join(name);
        if fs::symlink_metadata(&path)
            .is_ok_and(|m| !m.is_file() || m.permissions().mode() & 0o077 != 0)
        {
            return Err("REVIEW_LOCK_PRIVATE stage=review_state".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        if wait {
            file.lock()?;
        } else {
            file.try_lock()
                .map_err(|_| "REVIEW_BUSY stage=review_state")?;
        }
        Ok(Self {
            directory,
            _lock: file,
        })
    }
    pub fn load(&self) -> Result<Option<State>> {
        read(&self.directory.join("state.json"))
    }
    pub fn save(&self, state: &State) -> Result<()> {
        super::source_store::save(&self.directory, "state.json", &serde_json::to_value(state)?)
    }
    pub fn read(worker: &str) -> Result<Option<State>> {
        read(&Self::directory(worker)?.join("state.json"))
    }
}
pub fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    use std::io::Read;
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("REVIEW_STATE_PRIVATE stage=review_state".into());
    }
    let mut bytes = vec![];
    File::open(path)?.read_to_end(&mut bytes)?;
    let root = path
        .ancestors()
        .find(|p| p.join("sources").is_dir())
        .unwrap_or(path.parent().unwrap())
        .join("sources");
    let value = super::source_store::expand(serde_json::from_slice(&bytes)?, &root)?;
    serde_json::from_value(value)
        .map(Some)
        .map_err(|_| "REVIEW_STATE_JSON stage=review_state".into())
}
