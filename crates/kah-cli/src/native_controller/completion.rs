use super::*;
use kah_core::unattended::CompletionEvidence;
use kah_run::{access, verification};
use serde_json::json;

impl Run<'_> {
    pub(super) fn checkpoint(&self) -> Result<Worktree> {
        let saved = self.directory.join("candidate.json");
        if self.record()?.control.status() == RunStatus::Completed {
            return Ok(Worktree::load(&saved)?);
        }
        let nonce = session::now_ms();
        let bundle = self.directory.join(format!("checkpoint-{nonce}.bundle"));
        let commit = self.tree.checkpoint_bundle(&self.executor, &bundle)?;
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
        if !native.synchronized() || self.history.is_none() {
            return Ok(());
        }
        if !self.history.as_mut().unwrap().completed()? {
            return Ok(());
        }
        let snapshot = self.history.as_mut().unwrap().snapshot()?;
        if Some(&snapshot.action_digest) == native.processed_digest.as_ref() {
            return Ok(());
        }
        let projection = self.refresh(state, &snapshot)?;
        let sequence = record.control.inputs().last().unwrap().sequence;
        let candidate = self.checkpoint()?;
        let commit = self.record()?.control.last_checkpoint().unwrap().to_owned();
        let (tests, changed) = self.verify(&candidate, &commit, sequence)?;
        let completion = if tests.success && !changed {
            let activity = self.history.as_mut().unwrap().completion_activity()?;
            Some(self.judge_candidate(state, &projection, &activity, &candidate, &tests)?)
        } else {
            None
        };
        let pane = self.pane.inspect()?;
        let current = self.synchronize(&pane)?;
        let unchanged = current.is_some_and(|s| {
            s.action_digest.as_ref() == Some(&snapshot.action_digest) && s.inflight.is_none()
        }) && matches!(pane["agent_status"].as_str(), Some("idle" | "done"));
        let proof = CompletionEvidence {
            ledger_user_sequence: sequence,
            commit: commit.clone(),
            tests_passed: tests.success && !changed,
            outcomes_met: unchanged && completion.as_ref().is_some_and(|c| c.complete),
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
        let feedback = format!(
            "[kah-builder] Controller verification for commit {commit}: {}. This is automated evidence, not new user intent. Continue the user's active task if it is incomplete; preserve all user constraints.",
            serde_json::to_string(
                &json!({"testSuccess":tests.success,"candidateChanged":changed,"output":tests.output})
            )?
        );
        self.tests = Some(tests);
        self.continue_work(&current, sequence, unchanged, &feedback)
    }
    fn refresh(
        &self,
        state: &session::State,
        snapshot: &kah_core::observation::ObservationSnapshot,
    ) -> Result<kah_core::projection::ActiveProjection> {
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
        let tree = candidate.verification_snapshot(
            &candidate
                .path
                .parent()
                .unwrap()
                .join(format!("verify-{}", session::now_ms())),
            commit,
        )?;
        let uid = kah_run::access::service_uid()?;
        access::traverse(candidate.path.parent().unwrap(), uid)?;
        access::share_tree(&tree.path, uid, true)?;
        let project = Project {
            domain: Domain::Research,
            root: tree.path.clone(),
            relative_cwd: Default::default(),
            needs_creation: false,
        };
        let executor = crate::native_setup::executor(
            self.home,
            &project,
            LaunchOptions {
                online: self.online,
                yolo: false,
            },
            Some(&tree.git_dir),
        )?
        .with_read_only_mounts(&[
            (tree.git_dir.clone(), tree.git_dir.clone()),
            (tree.path.join(".git"), tree.path.join(".git")),
        ])?;
        let script = self.settings.test.clone();
        let log = self
            .directory
            .join(format!("verification-{}.log", session::now_ms()));
        let result = verification::execute_checked(
            &executor,
            &script,
            commit,
            Duration::from_secs(120),
            &log,
            || {
                self.verification_interrupted(sequence)
                    .map_err(|e| e.to_string())
            },
        );
        stop_executors(&tree.path)?;
        let changed = tree.checkpoint("chore: inspect verification side effects")? != commit;
        tree.remove_verification()?;
        Ok((result?, changed))
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
