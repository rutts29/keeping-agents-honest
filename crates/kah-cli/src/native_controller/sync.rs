use super::*;
use serde_json::Value;

impl Run<'_> {
    pub(super) fn attention(&self, pane: &Value) -> Result<()> {
        let record = self.record()?;
        let native = record.native.as_ref().unwrap();
        let reason = crate::run_pane::waiting_reason(
            pane,
            native.source_cursor > 0,
            self.started.elapsed().as_secs(),
        );
        if reason == native.waiting.as_deref() {
            return Ok(());
        }
        if let Some(reason) = reason {
            self.pane.notify_waiting(self.id, reason)?;
        }
        self.update(|r| {
            r.native.as_mut().unwrap().waiting = reason.map(str::to_owned);
            Ok(())
        })?;
        Ok(())
    }
    fn bind_session(&self, pane: &Value) -> Result<()> {
        let before = self.record()?;
        let native = before
            .native
            .as_ref()
            .ok_or("RUN_NATIVE_STATE stage=run_sync")?;
        if let Some(id) = pane["agent_session"]["value"].as_str() {
            if pane["agent"] != native.provider
                || native.session.as_deref().is_some_and(|old| old != id)
            {
                return Err(
                    "RUN_PANE_REPLACED stage=run_sync: native session identity changed".into(),
                );
            }
            if native.session.is_none() {
                self.update(|r| {
                    r.native.as_mut().unwrap().session = Some(id.into());
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
    pub(super) fn synchronize(&mut self, pane: &Value) -> Result<Option<session::State>> {
        self.bind_session(pane)?;
        let record = self.record()?;
        let native = record.native.as_ref().unwrap();
        let state = native
            .session
            .as_ref()
            .map(|id| {
                session::Store::read(
                    &RunStore::default_state_root().map_err(|e| e.to_string())?,
                    id,
                )
            })
            .transpose()?
            .flatten();
        let mut texts = vec![];
        if let Some(state) = &state {
            if state.binding.project != self.tree.path {
                return Err("RUN_OBSERVER_SCOPE stage=run_sync".into());
            }
            let base = RunStore::default_state_root()?;
            let store =
                session::Store::open(&base, &state.binding.session, Duration::from_secs(1))?;
            texts = store.sources()?.into_iter().map(|i| i.text).collect();
            drop(store);
            if self.history.is_none() && !texts.is_empty() {
                self.history = Some(History::open(
                    self.home,
                    &self.directory,
                    &native.provider,
                    &self.tree.path,
                    &state.binding.session,
                )?);
            }
        }
        let tokens = if self.sampled.elapsed() >= Duration::from_secs(2) {
            self.sampled = Instant::now();
            self.history
                .as_mut()
                .map(History::tokens)
                .transpose()?
                .unwrap_or(native.native_tokens)
        } else {
            native.native_tokens
        };
        let observer = state
            .as_ref()
            .map(|s| s.usage.input_tokens.saturating_add(s.usage.output_tokens))
            .unwrap_or(native.observer_tokens);
        let elapsed = self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.clock = Instant::now();
        self.update(|r| {
            let n = r.native.as_mut().unwrap();
            if state.is_some() {
                n.sources(&mut r.control, &texts)?;
            }
            n.account(&mut r.control, tokens, observer, elapsed);
            Ok(())
        })?;
        Ok(state)
    }
    pub(super) fn deliver(&self, pane: &Value) -> Result<()> {
        let record = self.record()?;
        let native = record.native.as_ref().unwrap();
        if native.pending.is_empty()
            || !native.submitted.is_empty()
            || pane["agent_status"] == "blocked"
        {
            return Ok(());
        }
        let mut text = None;
        self.update(|r| {
            text = r.native.as_mut().unwrap().begin_delivery(&r.control)?;
            Ok(())
        })?;
        if let Some(text) = text {
            // The saved 'sending' receipt lets history acknowledge a prompt
            // even if the controller dies after native submission.
            self.pane.prompt(&text)?;
            self.update(|r| r.native.as_mut().unwrap().delivered())?;
        }
        Ok(())
    }
}
