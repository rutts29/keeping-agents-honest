use super::*;
use kah_core::unattended::CompletionEvidence;
use kah_run::verification;
use serde_json::json;

impl Run<'_> {
    pub(super) fn capture_candidate(&self) -> Result<Worktree> {
        let saved = self.directory.join("candidate.json");
        if self.record()?.control.status() == RunStatus::Completed {
            return Ok(Worktree::load(&saved)?);
        }
        let nonce = session::now_ms();
        let bundle = self.directory.join(format!("delivery-{nonce}.bundle"));
        let commit = self.tree.bundle_head(&self.executor, &bundle)?;
        let candidate = if saved.exists() {
            Worktree::load(&saved)?
        } else {
            let mut candidate = Worktree::create(
                &self.record()?.project,
                &self.tree.path.parent().unwrap().join("candidate"),
                self.id,
            )?;
            candidate.base_commit = self.tree.base_commit.clone();
            candidate.save(&saved)?;
            candidate
        };
        candidate.import_bundle(&bundle, &commit)?;
        candidate.export_branch()?;
        self.update(|r| r.control.checkpoint(commit.clone()))?;
        std::fs::remove_file(bundle)?;
        Ok(candidate)
    }
    pub(super) fn assess(&mut self, state: &session::State) -> Result<()> {
        let record = self.record()?;
        let native = record.native.as_ref().unwrap();
        let Some(snapshot) = self.fresh_completion(native)? else {
            return Ok(());
        };
        let projection = self.refresh(state, &snapshot)?;
        let sequence = record.control.inputs().last().unwrap().sequence;
        let candidate = self.capture_candidate()?;
        let commit = self.record()?.control.last_checkpoint().unwrap().to_owned();
        let Some((tests, changed, checkpoint_passed)) = self.reviewed_verification(
            state,
            &projection,
            &snapshot,
            &candidate,
            &commit,
            sequence,
        )?
        else {
            return Ok(());
        };
        let completion = if tests.success && !changed {
            let activity = self.history.as_mut().unwrap().completion_activity()?;
            Some(self.judge_candidate(state, &projection, &activity, &candidate, &tests)?)
        } else {
            None
        };
        let unchanged = self.completion_unchanged(&commit, &snapshot)?;
        let proof = CompletionEvidence {
            ledger_user_sequence: sequence,
            commit: commit.clone(),
            tests_passed: tests.success && !changed,
            outcomes_met: unchanged
                && checkpoint_passed
                && completion.as_ref().is_some_and(|c| c.complete),
        };
        let current = self.update(|r| {
            if r.native.as_ref().unwrap().synchronized() {
                r.control.complete(&proof)?;
            }
            r.native.as_mut().unwrap().processed_digest = Some(snapshot.action_digest.clone());
            Ok(())
        })?;
        crate::unattended_report::save(
            &self.directory,
            &format!("completion-{commit}-{sequence}-{}.json", session::now_ms()),
            &json!({"proof":{"commit":commit,"userSequence":sequence,"tests":tests,"unchanged":unchanged},"judgments":completion.map(|c|c.judgments),"completed":current.control.status() == RunStatus::Completed}),
        )?;
        self.finish_assessment(
            &current,
            sequence,
            unchanged,
            tests,
            commit,
            (checkpoint_passed, changed),
        )
    }
    fn fresh_completion(
        &mut self,
        native: &kah_run::native::Native,
    ) -> Result<Option<kah_core::observation::ObservationSnapshot>> {
        if !native.synchronized() {
            return Ok(None);
        }
        let Some(history) = self.history.as_mut() else {
            return Ok(None);
        };
        if !history.completed()? {
            return Ok(None);
        }
        let snapshot = history.snapshot()?;
        Ok((Some(&snapshot.action_digest) != native.processed_digest.as_ref()).then_some(snapshot))
    }
    fn completion_unchanged(
        &mut self,
        commit: &str,
        snapshot: &kah_core::observation::ObservationSnapshot,
    ) -> Result<bool> {
        let pane = self.pane.inspect()?;
        let current = self.synchronize(&pane)?;
        Ok(self.tree.sandbox_state(&self.executor)?.0 == commit
            && current.is_some_and(|s| {
                s.action_digest.as_ref() == Some(&snapshot.action_digest) && s.inflight.is_none()
            })
            && matches!(pane["agent_status"].as_str(), Some("idle" | "done")))
    }
    fn finish_assessment(
        &mut self,
        current: &Record,
        sequence: u64,
        unchanged: bool,
        tests: verification::Receipt,
        commit: String,
        review_flags: (bool, bool),
    ) -> Result<()> {
        let (checkpoint_passed, changed) = review_flags;
        let uncommitted = self.tree.sandbox_state(&self.executor)?.1;
        let feedback = format!(
            "[kah-builder] Verification of your HEAD commit {commit}: {}. Only this SHA is delivered; uncommitted changes are NOT delivered. Commit only intended deliverable work, keeping confidential/personal/internal files out of commits. If the user's instructions prohibit committing necessary code, explain the conflict and ask. This is automated evidence, not new user intent.",
            serde_json::to_string(
                &json!({"testSuccess":tests.success,"checkpointPassed":checkpoint_passed,"candidateChanged":changed,"uncommittedChangesPresent":uncommitted,"output":tests.output})
            )?
        );
        self.tests = Some(tests);
        self.continue_work(current, sequence, unchanged, &feedback)
    }
    fn reviewed_verification(
        &mut self,
        state: &session::State,
        projection: &kah_core::projection::ActiveProjection,
        snapshot: &kah_core::observation::ObservationSnapshot,
        candidate: &Worktree,
        commit: &str,
        sequence: u64,
    ) -> Result<Option<(verification::Receipt, bool, bool)>> {
        let native = self.record()?.native.unwrap();
        let key = crate::completion_review::Key {
            instance: native.instance.ok_or("RUN_INSTANCE stage=completion")?,
            commit: commit.into(),
            user_sequence: sequence,
            action_digest: snapshot.action_digest.clone(),
            command: self.settings.test.clone(),
            online: self.online,
        };
        let directory = self.directory.clone();
        let verified = crate::completion_review::verified(&directory, &key, || {
            self.verify(candidate, commit, sequence)
        })?;
        if !verified.receipt.success || verified.changed {
            return Ok(Some((verified.receipt, verified.changed, false)));
        }
        let status = crate::completion_checkpoint::request(
            &directory,
            &state.binding.session,
            &key,
            projection.revision,
            &verified.receipt,
        )?;
        match status {
            crate::completion_checkpoint::Status::Pending => {
                self.tests = Some(verified.receipt);
                Ok(None)
            }
            crate::completion_checkpoint::Status::Passed => {
                Ok(Some((verified.receipt, verified.changed, true)))
            }
            crate::completion_checkpoint::Status::Failed(_) => {
                Ok(Some((verified.receipt, verified.changed, false)))
            }
        }
    }
    fn refresh(
        &self,
        state: &session::State,
        snapshot: &kah_core::observation::ObservationSnapshot,
    ) -> Result<kah_core::projection::ActiveProjection> {
        let record = self.record()?;
        let native = record
            .native
            .as_ref()
            .ok_or("DISPATCH_NATIVE stage=completion")?;
        crate::dispatch::apply(
            &state.binding,
            self.id,
            native
                .instance
                .as_deref()
                .ok_or("DISPATCH_INSTANCE stage=completion")?,
            &native.provider,
        )?;
        // Native writers may append the final assistant record after Stop.
        // Once idle, refresh the existing observer from provider-owned history;
        // its intake remains the only authority and its Jev questions are unchanged.
        let outcome = kah_observer::engine::observe_snapshot(
            &state.binding,
            "run:idle",
            false,
            kah_core::observation::ObservationSnapshot {
                users: snapshot.users.clone(),
                actions: snapshot.actions.clone(),
                action_digest: snapshot.action_digest.clone(),
            },
        )?;
        if let Some(gap) = outcome.gap {
            return Err(format!("RUN_OBSERVER_GAP stage=completion: {gap}").into());
        }
        outcome
            .projection
            .ok_or_else(|| "RUN_INTENT_MISSING stage=completion".into())
    }
    fn judge_candidate(
        &self,
        state: &session::State,
        projection: &kah_core::projection::ActiveProjection,
        actions: &[serde_json::Value],
        candidate: &Worktree,
        tests: &verification::Receipt,
    ) -> Result<crate::unattended_observer::Completion> {
        let settings = Settings {
            test: self.settings.test.clone(),
            allow_hosts: vec![],
            observer: state.binding.clone(),
        };
        Ok(crate::unattended_observer::complete_candidate(
            self.home,
            &settings,
            projection,
            actions,
            candidate,
            tests,
            self.online,
        )?)
    }
    fn continue_work(
        &mut self,
        record: &Record,
        sequence: u64,
        unchanged: bool,
        feedback: &str,
    ) -> Result<()> {
        if record.control.status() != RunStatus::Running
            || !unchanged
            || !record
                .native
                .as_ref()
                .unwrap()
                .current(&record.control, sequence)
        {
            return Ok(());
        }
        self.unsuccessful += 1;
        if self.unsuccessful >= 2 {
            return Err("RUN_NO_COMPLETION stage=completion: inspect the native session and report, then steer before resuming".into());
        }
        self.pane.prompt(feedback)?;
        Ok(())
    }
    fn verify(
        &mut self,
        candidate: &Worktree,
        commit: &str,
        sequence: u64,
    ) -> Result<(verification::Receipt, bool)> {
        let home = self.home;
        let online = self.online;
        let script = self.settings.test.clone();
        let log = self
            .directory
            .join(format!("verification-{}.log", session::now_ms()));
        crate::exact_verification::execute(
            home,
            candidate,
            commit,
            LaunchOptions {
                online,
                yolo: false,
            },
            &script,
            &log,
            || {
                self.verification_interrupted(sequence)
                    .map_err(|e| e.to_string())
            },
        )
    }

    fn verification_interrupted(&mut self, sequence: u64) -> Result<bool> {
        let elapsed = self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.clock = Instant::now();
        let record = self.update(|r| {
            r.control.account(0, 0, elapsed);
            Ok(())
        })?;
        Ok(record.control.status() != RunStatus::Running
            || !record
                .native
                .as_ref()
                .unwrap()
                .current(&record.control, sequence)
            || self.pane.inspect()?["agent_status"] == "working")
    }
}
