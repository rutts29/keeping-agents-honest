use super::Store;
use crate::inherited::{Inbox, Quote};
use kah_core::intent_history::SourceProvider;
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

impl Store {
    /// Receipt intake uses a separate brief I/O lock, never the judge's lease.
    pub fn accept_inherited(
        base: &std::path::Path,
        session: &str,
        provider: SourceProvider,
        quote: Quote,
    ) -> Result<(), String> {
        Self::open_lock(
            base,
            session,
            std::time::Duration::from_millis(200),
            "inherit.lock",
        )?
        .inherit(provider, quote)
    }
    pub fn inherited(&self) -> Result<Option<Inbox>, String> {
        let path = self.path.join("inherited.json");
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(failure(e)),
        };
        if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 || meta.len() > 512 * 1024 {
            return Err("INHERITED_PRIVATE stage=intake".into());
        }
        let inbox: Inbox = serde_json::from_slice(&fs::read(path).map_err(failure)?)
            .map_err(|_| "INHERITED_JSON stage=intake")?;
        inbox.validate()?;
        Ok(Some(inbox))
    }
    fn inherit(&mut self, provider: SourceProvider, quote: Quote) -> Result<(), String> {
        quote.validate()?;
        let mut inbox = self.inherited()?.unwrap_or(Inbox {
            provider: provider.clone(),
            quotes: vec![],
        });
        if inbox.provider != provider {
            return Err("INHERITED_PROVIDER stage=intake".into());
        }
        if let Some(old) = inbox.find(&quote.key()) {
            if old.source == quote.source && old.text == quote.text {
                return Ok(());
            }
            return Err("INHERITED_REWRITE stage=intake".into());
        }
        inbox.quotes.push(quote);
        inbox.validate()?;
        let bytes = serde_json::to_vec(&inbox).map_err(|_| "INHERITED_JSON stage=intake")?;
        if bytes.len() > 512 * 1024 {
            return Err("INHERITED_BOUND stage=intake".into());
        }
        let temporary = self.path.join(format!("inherited-{}.tmp", super::now_ms()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(failure)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(failure)?;
        fs::rename(temporary, self.path.join("inherited.json")).map_err(failure)?;
        fs::File::open(&self.path)
            .and_then(|f| f.sync_all())
            .map_err(failure)
    }
}
fn failure(error: std::io::Error) -> String {
    format!("INHERITED_IO stage=intake: {error}")
}
