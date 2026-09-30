//! Small coordinator boundary between session inspection and durable run state.

use std::path::Path;

use sha2::{Digest, Sha256};

use kah_core::intent_history::SourceProvider;
use kah_core::run_store::{
    RecoveredRun, RunId, RunStore, RunStoreError, SourceCursor, SourceRecordDigest,
};
use kah_core::session_bundle::NeutralSessionBundle;

pub struct InspectSource {
    run_id: RunId,
    cursor: SourceCursor,
}

impl InspectSource {
    pub fn from_bundle(bundle: &NeutralSessionBundle) -> Result<Self, RunStoreError> {
        let cursor = SourceCursor::new(
            bundle.source().provider(),
            bundle.source().session_ref_private(),
            bundle.source().repository_digest(),
            bundle
                .records()
                .iter()
                .map(|record| {
                    SourceRecordDigest::with_role(
                        record.id(),
                        record.content_digest(),
                        record.role(),
                    )
                })
                .collect(),
        )?;
        let mut digest = Sha256::new();
        digest.update(b"kah.user-controlled-run.v3\0");
        digest.update(provider_name(cursor.provider()).as_bytes());
        digest.update([0]);
        digest.update(cursor.session_locator_private().as_bytes());
        digest.update([0]);
        digest.update(cursor.repository_digest().as_bytes());
        let hex = format!("{:x}", digest.finalize());
        let run_id = RunId::parse(format!("run-{}", &hex[..24]))?;
        Ok(Self { run_id, cursor })
    }

    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    pub fn cursor(&self) -> &SourceCursor {
        &self.cursor
    }

    pub fn open(&self, repository: &Path) -> Result<Option<LockedInspectRun>, RunStoreError> {
        match RunStore::open_default(repository, self.run_id.as_str()) {
            Ok(store) => {
                let recovered = store.recover()?;
                Ok(Some(LockedInspectRun { store, recovered }))
            }
            Err(RunStoreError::RunNotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn create(
        &self,
        repository: &Path,
        history: &kah_core::intent_history::History,
        projection: &kah_core::projection::ActiveProjection,
    ) -> Result<RunStore, RunStoreError> {
        RunStore::create_with_source_cursor(
            RunStore::default_state_root()?,
            repository,
            self.run_id.as_str(),
            history,
            projection,
            &self.cursor,
        )
    }
}

pub struct LockedInspectRun {
    pub store: RunStore,
    pub recovered: RecoveredRun,
}

fn provider_name(provider: &SourceProvider) -> &'static str {
    match provider {
        SourceProvider::Codex => "codex",
        SourceProvider::Claude => "claude",
        SourceProvider::Bundle => "bundle",
    }
}
