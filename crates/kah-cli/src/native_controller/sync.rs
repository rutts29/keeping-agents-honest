use super::*;
use serde_json::Value;

impl Run<'_> {
    pub(super) fn attention(&mut self, pane: &Value) -> Result<()> {
        let record = self.record()?;
        let native = record.native.as_ref().unwrap();
        self.ready |= crate::run_pane::accepts_confirmed_input(
            pane,
            native,
            crate::native_binding::confirmed(native),
        );
        let reason =
            crate::run_pane::waiting_reason(pane, self.ready, self.started.elapsed().as_secs());
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
    fn bind_session(&mut self, pane: &Value) -> Result<()> {
        let before = self.record()?;
        let native = before
            .native
            .as_ref()
            .ok_or("RUN_NATIVE_STATE stage=run_sync")?;
        if native.provider == "claude" {
            return self.bind_claude(pane, native);
        }
        if let Some(id) = pane["agent_session"]["value"].as_str() {
            // Native resume can report a bootstrap identity before restoring the
            // requested session. Never deliver to it; allow startup to settle.
            if !self.ready
                && native.session.is_some()
                && pane["agent"] == native.provider
                && native.session.as_deref() != Some(id)
                && self.started.elapsed() < Duration::from_secs(15)
            {
                return Ok(());
            }
            if pane["agent"] != native.provider
                || native.session.as_deref().is_some_and(|old| old != id)
            {
                return Err(format!(
                    "RUN_PANE_REPLACED stage=run_sync: expected {} {:?}, observed {} {id}",
                    native.provider, native.session, pane["agent"]
                )
                .into());
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
        }
        if self.history.is_none()
            && let Some(id) = native.session.as_deref()
            && (!texts.is_empty()
                || native.provider == "codex" && self.sampled.elapsed() >= Duration::from_secs(2))
        {
            self.open_history(native, id)?;
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
        if !native.can_deliver(pane["agent_status"] == "working")
            || !crate::run_pane::accepts_confirmed_input(
                pane,
                native,
                crate::native_binding::confirmed(native),
            )
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
    fn bind_claude(&mut self, pane: &Value, native: &kah_run::native::Native) -> Result<()> {
        let Some(c) = &native.confirmation else {
            return Ok(());
        };
        if native.instance.as_deref() != Some(&c.instance) {
            return Ok(());
        }
        if native.session.as_deref().is_some_and(|id| id != c.session) {
            if c.source == "startup" && !self.ready {
                return Ok(());
            }
            return Err(format!("RUN_NATIVE_SESSION_REPLACED stage=run_sync: expected Claude {:?}; observed SessionStart source={} id={} transcript={}",native.session,c.source,c.session,c.transcript.display()).into());
        }
        if !std::fs::metadata(&c.transcript).is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Ok(());
        }
        if native.session.is_none() {
            self.update(|r| {
                r.native.as_mut().unwrap().session = Some(c.session.clone());
                Ok(())
            })?;
            crate::observer_review::tags::schedule(&c.session)?;
        }
        if self.ready
            && pane["agent"]
                .as_str()
                .is_some_and(|agent| agent != "claude")
        {
            return Err(format!("RUN_PANE_REPLACED stage=run_sync: native SessionStart confirms Claude {}; Herdr agent={}",c.session,pane["agent"]).into());
        }
        if pane["agent_session"]["value"].as_str() != Some(&c.session) {
            let note = pane["agent_session"].to_string();
            if self.identity_note.as_deref() != Some(&note) {
                crate::unattended_report::save(
                    &self.directory,
                    "identity-advisory.json",
                    &serde_json::json!({"expected":{"source":"native SessionStart","session":c.session,"eventSource":c.source,"transcript":c.transcript,"name":c.name},"observedHerdr":pane["agent_session"],"agent":pane["agent"]}),
                )?;
                self.identity_note = Some(note);
            }
        }
        Ok(())
    }
    fn open_history(&mut self, native: &kah_run::native::Native, session: &str) -> Result<()> {
        let opened = History::open(
            self.home,
            &self.directory,
            &native.provider,
            &self.tree.path,
            session,
        );
        let mut history = match opened {
            Err(e)
                if native.provider == "codex"
                    && native.source_cursor == 0
                    && crate::native_resume::missing_thread(e.as_ref(), session) =>
            {
                return Ok(());
            }
            result => result?,
        };
        if native.provider == "codex"
            && native.source_cursor == 0
            && let Some(name) = &native.name
        {
            history.set_codex_name(name)?;
        }
        self.history = Some(history);
        self.sampled = Instant::now() - Duration::from_secs(2);
        Ok(())
    }
}
