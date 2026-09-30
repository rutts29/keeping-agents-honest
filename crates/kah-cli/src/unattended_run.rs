//! One foreground coordinator. The native agent is ephemeral; all tools are remote.
use crate::{
    Result,
    unattended_observer::{self as observer, Settings, WorkerReply},
};
use kah_core::{
    run_store::RunStore,
    unattended::{CompletionEvidence, RunStatus},
};
use kah_observer::{engine, session};
use kah_run::{
    attempt,
    lease::Lease,
    runtime::Runtime,
    store::{Record, Store},
    verification,
    worktree::Worktree,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub fn execute(home: &Path, id: &str, resume: bool) -> Result<()> {
    let base = RunStore::default_state_root()?.join("runs");
    let _lease = Lease::acquire(&base, id)?;
    let directory = Store::path(&base, id)?;
    let settings = crate::unattended_setup::settings(&directory)?;
    let tree = Worktree::load(&directory.join("worktree.json"))?;
    let record = Store::read(&base, id)?;
    activate(&mut record.clone(), resume)?;
    let mut runtime = Runtime::prepare(home, &directory, id, &record, &tree)?;
    let _fetch = kah_run::fetch_runtime::enable(&mut runtime, &record, &settings.allow_hosts)?;
    runtime.stop_executor(id)?;
    if resume {
        tree.restore_checkpoint(
            record
                .control
                .last_checkpoint()
                .ok_or("RUN_CHECKPOINT_MISSING stage=resume")?,
        )?;
    }
    let mut run = Run {
        home,
        base,
        id,
        directory,
        settings,
        tree,
        runtime,
        tests: None,
        feedback: String::new(),
    };
    run.update(|record| activate(record, resume))?;
    run.tree.export_branch()?;
    println!(
        "Run {id}; branch kah/{id}; source commit {}",
        run.tree.base_commit
    );
    let result = run.work();
    let cleanup = run.runtime.stop_executor(id);
    if result.is_err() || cleanup.is_err() {
        run.update(|r| {
            r.control.stop();
            Ok(())
        })?;
    }
    let problem = result
        .as_ref()
        .err()
        .map(ToString::to_string)
        .or_else(|| cleanup.as_ref().err().cloned());
    crate::unattended_report::write(
        &run.directory,
        id,
        &run.record()?,
        &run.tree,
        run.tests.as_ref(),
        problem.as_deref(),
    )?;
    cleanup?;
    result
}
fn activate(record: &mut Record, resume: bool) -> std::result::Result<(), String> {
    if resume
        && matches!(
            record.control.status(),
            RunStatus::Running | RunStatus::Stopping
        )
    {
        record.control.stop();
    }
    if record.control.status() == RunStatus::Created {
        record.control.start()
    } else if resume {
        record.control.resume()
    } else {
        Err("RUN_ALREADY_STARTED stage=run_start".into())
    }
}
struct Run<'a> {
    home: &'a Path,
    base: PathBuf,
    id: &'a str,
    directory: PathBuf,
    settings: Settings,
    tree: Worktree,
    runtime: Runtime,
    tests: Option<verification::Receipt>,
    feedback: String,
}
impl Run<'_> {
    fn record(&self) -> Result<Record> {
        Ok(Store::read(&self.base, self.id)?)
    }
    fn update(
        &self,
        action: impl FnOnce(&mut Record) -> std::result::Result<(), String>,
    ) -> Result<Record> {
        let mut store = Store::open(&self.base, self.id)?;
        let mut record = store.load()?;
        let result = action(&mut record);
        store.save(&record)?;
        result?;
        Ok(record)
    }
    fn work(&mut self) -> Result<()> {
        let mut unsuccessful = 0;
        while self.record()?.control.status() == RunStatus::Running {
            let record = self.record()?;
            let sequence = record.control.inputs().last().unwrap().sequence;
            let observation = self.observe(&record, vec![], "before-attempt")?;
            if self.record()?.control.status() != RunStatus::Running {
                break;
            }
            if self.record()?.control.inputs().last().unwrap().sequence != sequence {
                continue;
            }
            let prompt = self.prompt(&record, observation.projection.as_ref());
            let mut command = self.runtime.command(&record);
            command
                .arg("--output-schema")
                .arg(self.directory.join("worker-schema.json"));
            let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let outcome = attempt::run(
                &self.base,
                self.id,
                command,
                &prompt,
                &self.directory.join(format!("attempt-{nonce}.log")),
            );
            self.runtime.stop_executor(self.id)?;
            let commit = self.checkpoint()?;
            let outcome = outcome?;
            if self.record()?.control.status() != RunStatus::Running {
                break;
            }
            if outcome.stopped.as_deref() == Some("RUN_STEER") {
                unsuccessful = 0;
                continue;
            }
            if outcome.stopped.is_some() {
                break;
            }
            if !outcome.success {
                return Err("RUN_NATIVE_FAILED stage=run_attempt".into());
            }
            if self.assess(sequence, &commit, outcome.actions)? {
                return Ok(());
            }
            if self.record()?.control.inputs().last().unwrap().sequence != sequence {
                unsuccessful = 0;
                continue;
            }
            unsuccessful += 1;
            if unsuccessful >= 2 {
                return Err("RUN_NO_COMPLETION stage=run_loop: two attempts did not demonstrate completion; inspect the report and steer before resuming".into());
            }
        }
        self.update(|r| {
            if r.control.status() == RunStatus::Stopping {
                r.control.stop();
            }
            Ok(())
        })?;
        Ok(())
    }
    fn checkpoint(&self) -> Result<String> {
        let commit = self.tree.checkpoint("chore: checkpoint unattended work")?;
        self.update(|r| r.control.checkpoint(commit.clone()))?;
        self.tree.export_branch()?;
        Ok(commit)
    }
    fn prompt(
        &self,
        record: &Record,
        projection: Option<&kah_core::projection::ActiveProjection>,
    ) -> String {
        let mut prompt = observer::worker_prompt(record, projection, &self.feedback);
        if !self.settings.allow_hosts.is_empty() {
            prompt.push_str(&format!("\nResearch network policy: raw sockets remain disabled. Use `/opt/kah __fetch URL` for read-only HTTPS GET from these exact hosts on port 443: {:?}. Redirects, credentials and other hosts are refused.", self.settings.allow_hosts));
        }
        prompt
    }
    fn observe(&self, record: &Record, actions: Vec<Value>, turn: &str) -> Result<engine::Outcome> {
        let state_base = RunStore::default_state_root()?;
        let before = session::Store::read(&state_base, self.id)?
            .map(|s| s.usage)
            .unwrap_or_default();
        let started = Instant::now();
        let result = observer::observe(record, &self.settings, turn, actions);
        let after = session::Store::read(&state_base, self.id)?
            .map(|s| s.usage)
            .unwrap_or_default();
        self.update(|r| {
            r.control.account(
                after.input_tokens.saturating_sub(before.input_tokens),
                after.output_tokens.saturating_sub(before.output_tokens),
                started.elapsed().as_millis() as u64,
            );
            Ok(())
        })?;
        Ok(result?)
    }
    fn assess(&mut self, sequence: u64, commit: &str, actions: Vec<Value>) -> Result<bool> {
        let record = self.record()?;
        let observation = self.observe(&record, actions.clone(), commit)?;
        if self.record()?.control.status() != RunStatus::Running {
            return Ok(true);
        }
        if self.record()?.control.inputs().last().unwrap().sequence != sequence {
            return Ok(false);
        }
        let reply = WorkerReply::from_actions(&actions)?;
        self.feedback = observation
            .reminder
            .map(|reminder| reminder.text)
            .unwrap_or_default();
        if reply.status == "external_action_required" {
            self.update(|r| r.control.block_external(&reply.question))?;
            return Ok(true);
        }
        if let Some(gap) = observation.gap {
            return Err(format!("RUN_OBSERVER_GAP stage=completion: {gap}").into());
        }
        let projection = observation
            .projection
            .ok_or("RUN_INTENT_MISSING stage=completion")?;
        let (tests, changed) = self.verify(commit)?;
        self.update(|r| {
            r.control.account(0, 0, tests.elapsed_ms);
            Ok(())
        })?;
        self.feedback
            .push_str(&format!("\nVerification output: {}", tests.output));
        self.tests = Some(tests);
        if self.record()?.control.status() != RunStatus::Running {
            return Ok(true);
        }
        if changed || !self.tests.as_ref().unwrap().success {
            self.feedback = format!(
                "Tests failed or modified the candidate commit. {}",
                self.feedback
            );
            return Ok(false);
        }
        self.finish_completion(sequence, commit, &projection, &actions)
    }
    fn finish_completion(
        &mut self,
        sequence: u64,
        commit: &str,
        projection: &kah_core::projection::ActiveProjection,
        actions: &[Value],
    ) -> Result<bool> {
        let state_base = RunStore::default_state_root()?;
        let before = session::Store::read(&state_base, self.id)?
            .ok_or("RUN_OBSERVER_STATE stage=completion")?
            .usage;
        let started = Instant::now();
        let result = observer::complete(
            self.home,
            &self.settings,
            projection,
            actions,
            &self.tree.diff_evidence(24 * 1024)?,
            &self.tree.changed_paths()?,
            self.tests.as_ref().unwrap(),
        );
        let after = session::Store::read(&state_base, self.id)?
            .ok_or("RUN_OBSERVER_STATE stage=completion")?
            .usage;
        self.update(|r| {
            r.control.account(
                after.input_tokens.saturating_sub(before.input_tokens),
                after.output_tokens.saturating_sub(before.output_tokens),
                started.elapsed().as_millis() as u64,
            );
            Ok(())
        })?;
        let completion = result?;
        let proof = CompletionEvidence {
            ledger_user_sequence: sequence,
            commit: commit.into(),
            tests_passed: true,
            outcomes_met: completion.complete,
        };
        let current = self.update(|r| {
            r.control.complete(&proof)?;
            Ok(())
        })?;
        let receipt = json!({"commit":commit,"userSequence":sequence,"judgments":completion.judgments,"completed":current.control.status() == RunStatus::Completed});
        crate::unattended_report::save(
            &self.directory,
            &format!("completion-{commit}-{sequence}.json"),
            &receipt,
        )?;
        self.feedback
            .push_str(&format!("\nIndependent completion assessment: {receipt}"));
        Ok(current.control.status() != RunStatus::Running)
    }
    fn verify(&self, commit: &str) -> Result<(verification::Receipt, bool)> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let tree = self.tree.verification_snapshot(
            &self
                .tree
                .path
                .parent()
                .unwrap()
                .join(format!("verify-{nonce}")),
            commit,
        )?;
        let domain_name = self.record()?.domain;
        let domain = kah_sandbox::Domain::ALL
            .into_iter()
            .find(|d| d.as_str() == domain_name)
            .ok_or("RUN_DOMAIN stage=verification")?;
        let executor = self.runtime.verification_executor(domain, &tree, self.id)?;
        let result = verification::execute(
            &executor,
            &self.settings.test,
            commit,
            Duration::from_secs(120),
            &self.directory.join(format!("test-{nonce}.log")),
        );
        self.runtime.stop_executor(self.id)?;
        let changed = tree.checkpoint("chore: inspect verification side effects")? != commit;
        tree.remove_verification()?;
        Ok((result?, changed))
    }
}

pub fn handles(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "run")
        || args.first().is_some_and(|s| s == "resume")
            && args.get(1).is_some_and(|s| s.starts_with("run-"))
}
pub fn run_cli(args: Vec<String>) -> Result<()> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME_MISSING stage=run_arguments")?);
    if args[0] == "resume" {
        if args.len() != 2 {
            return Err("RUN_RESUME_ARGUMENTS stage=arguments: kah resume RUN".into());
        }
        return execute(&home, &args[1], true);
    }
    let options = crate::unattended_args::RunArguments::parse(args.into_iter().skip(1).collect())?;
    let cwd = std::env::current_dir().map_err(|e| format!("RUN_CWD stage=run_arguments: {e}"))?;
    let id = crate::unattended_setup::create(&home, &cwd, options)?;
    println!("Created {id}");
    execute(&home, &id, false)
}
