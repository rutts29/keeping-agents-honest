//! One pane, one full native process, one persistent run controller.
use crate::{
    Arguments, Result,
    native_child::{NativeChild, stop_executors},
    native_history::History,
    run_pane::Pane,
    unattended_observer::Settings,
};
use kah_core::{
    run_store::RunStore,
    unattended::{RunControl, RunStatus},
};
use kah_observer::session;
use kah_run::{
    lease::Lease,
    store::{Record, Store},
    worktree::Worktree,
};
use kah_sandbox::{Domain, Executor, LaunchOptions, Project};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
mod completion;
mod sync;

pub fn run_cli(arguments: &[String]) -> Result<()> {
    if arguments.len() != 3 || !matches!(arguments[2].as_str(), "start" | "resume") {
        return Err("RUN_CONTROLLER_ARGUMENTS stage=arguments".into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME_MISSING stage=run_launch")?);
    execute(&home, &arguments[1], arguments[2] == "resume")
}

pub fn launch(home: &Path, id: &str, resume: bool) -> Result<()> {
    let base = RunStore::default_state_root()?.join("runs");
    let record = Store::read(&base, id)?;
    if record.native.is_none() {
        return Err("RUN_LEGACY_WORKER stage=resume: this run predates native sessions".into());
    }
    activate(&mut record.control.clone(), resume)?;
    drop(Lease::acquire(&base, id)?);
    let mut command = std::process::Command::new(home.join(".local/bin/kah"));
    command
        .args(["__native-run", id, if resume { "resume" } else { "start" }])
        .current_dir(&record.worktree);
    let pane = Pane::start(&command)?;
    println!("Run {id}; native pane {}; kah status {id}", pane.id);
    println!(
        "Native startup may need your consent in that pane; kah will report waits through Herdr and kah status."
    );
    Ok(())
}

pub fn execute(home: &Path, id: &str, resume: bool) -> Result<()> {
    let base = RunStore::default_state_root()?.join("runs");
    let _lease = Lease::acquire(&base, id)?;
    {
        let mut store = Store::open_wait(&base, id)?;
        let mut record = store.load()?;
        record
            .native
            .as_mut()
            .ok_or("RUN_LEGACY_WORKER stage=run_launch")?
            .report_ready = Some(false);
        store.save(&record)?;
    }
    let result = execute_owned(home, base.clone(), id, resume);
    if let Err(error) = &result {
        let mut store = Store::open_wait(&base, id)?;
        let mut record = store.load()?;
        record.control.stop();
        if record
            .native
            .as_ref()
            .is_some_and(|n| n.report_ready != Some(true))
        {
            crate::unattended_report::save(
                store.directory(),
                "report.json",
                &serde_json::json!({"run":id,"result":record.control.status(),"worktree":record.worktree,"error":format!("RUN_NATIVE_SESSION stage=run_controller: {error}")}),
            )?;
        }
        if let Some(native) = record.native.as_mut() {
            native.report_ready = Some(true);
        }
        store.save(&record)?;
    }
    result
}
fn execute_owned(home: &Path, base: PathBuf, id: &str, resume: bool) -> Result<()> {
    if resume {
        crate::native_resume::prepare(home, &base, id)?;
    }
    let mut run = Run::open(home, base, id)?;
    let record = run.update(|r| {
        activate(&mut r.control, resume)?;
        r.native.as_mut().unwrap().pane = Some(run.pane.id.clone());
        r.native.as_mut().unwrap().herdr_socket =
            std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from);
        r.native.as_mut().unwrap().waiting = None;
        r.native.as_mut().unwrap().instance =
            Some(format!("{}-{}", session::now_ms(), std::process::id()));
        r.native.as_mut().unwrap().confirmation = None;
        r.native.as_mut().unwrap().report_ready = Some(false);
        if resume && r.native.as_ref().unwrap().session.is_some() {
            r.native.as_mut().unwrap().begin_delivery(&r.control)?;
        }
        Ok(())
    })?;
    stop_executors(&run.tree.path)?;
    let command = run.command(&record)?;
    let mut child = NativeChild::spawn(&command)?;
    if record.native.as_ref().unwrap().sending.is_some() {
        run.update(|r| r.native.as_mut().unwrap().delivered())?;
    }
    run.started = Instant::now();
    let result = run.work(&mut child);
    finish(&mut run, &mut child, result)
}
fn activate(control: &mut RunControl, resume: bool) -> std::result::Result<(), String> {
    if resume && matches!(control.status(), RunStatus::Running | RunStatus::Stopping) {
        control.stop();
    }
    if control.status() == RunStatus::Created {
        control.start()
    } else if resume {
        control.resume()
    } else {
        Err("RUN_ALREADY_STARTED stage=run_launch".into())
    }
}
fn finish(run: &mut Run<'_>, child: &mut NativeChild, result: Result<()>) -> Result<()> {
    let stopped = child.stop();
    let cleaned = stop_executors(&run.tree.path);
    let result = result.and(stopped).and(cleaned);
    if result.is_err() || run.record()?.control.status() == RunStatus::Stopping {
        run.update(|r| {
            r.control.stop();
            Ok(())
        })?;
    }
    let checkpoint = run.capture_candidate();
    let problem = result
        .as_ref()
        .err()
        .or(checkpoint.as_ref().err())
        .map(ToString::to_string);
    match checkpoint {
        Ok(candidate) => crate::unattended_report::write(
            &run.directory,
            run.id,
            &run.record()?,
            &candidate,
            run.tests.as_ref(),
            problem.as_deref(),
        )?,
        Err(error) => {
            crate::unattended_report::save(
                &run.directory,
                "report.json",
                &serde_json::json!({"run":run.id,"error":problem,"result":run.record()?.control.status(),"worktree":run.tree.path}),
            )?;
            run.publish_report()?;
            return Err(error);
        }
    }
    run.publish_report()?;
    result
}

struct Run<'a> {
    home: &'a Path,
    base: PathBuf,
    id: &'a str,
    directory: PathBuf,
    tree: Worktree,
    executor: Executor,
    settings: Settings,
    pane: Pane,
    history: Option<History>,
    clock: Instant,
    started: Instant,
    ready: bool,
    identity_note: Option<String>,
    sampled: Instant,
    tests: Option<kah_run::verification::Receipt>,
    unsuccessful: u8,
    followup_sample: Option<String>,
    online: bool,
}
impl<'a> Run<'a> {
    fn open(home: &'a Path, base: PathBuf, id: &'a str) -> Result<Self> {
        let directory = Store::path(&base, id)?;
        let settings = crate::unattended_setup::settings(&directory)?;
        let tree = Worktree::load(&directory.join("worktree.json"))?;
        let online = Store::read(&base, id)?
            .native
            .ok_or("RUN_NATIVE_STATE stage=run_launch")?
            .online;
        let project = Project {
            domain: Domain::Research,
            root: tree.path.clone(),
            relative_cwd: Default::default(),
            needs_creation: false,
        };
        let executor = crate::native_setup::executor(
            home,
            &project,
            LaunchOptions {
                online,
                yolo: false,
            },
            Some(&tree.git_dir),
        )?;
        let pane = Pane {
            id: std::env::var("HERDR_PANE_ID").map_err(|_| "RUN_PANE_REQUIRED stage=run_launch")?,
        };
        Ok(Self {
            home,
            base,
            id,
            directory,
            tree,
            executor,
            settings,
            pane,
            history: None,
            clock: Instant::now(),
            started: Instant::now(),
            ready: false,
            identity_note: None,
            sampled: Instant::now() - Duration::from_secs(2),
            tests: None,
            unsuccessful: 0,
            followup_sample: None,
            online,
        })
    }
    fn record(&self) -> Result<Record> {
        Ok(Store::read(&self.base, self.id)?)
    }
    fn update(
        &self,
        action: impl FnOnce(&mut Record) -> std::result::Result<(), String>,
    ) -> Result<Record> {
        let mut store = Store::open_wait(&self.base, self.id)?;
        let mut record = store.load()?;
        let result = action(&mut record);
        store.save(&record)?;
        result?;
        Ok(record)
    }
    fn publish_report(&self) -> Result<()> {
        self.update(|r| {
            r.native.as_mut().unwrap().report_ready = Some(true);
            Ok(())
        })?;
        Ok(())
    }
    fn command(&self, record: &Record) -> Result<std::process::Command> {
        let native = record
            .native
            .as_ref()
            .ok_or("RUN_NATIVE_STATE stage=run_launch")?;
        let mut args = Arguments {
            options: LaunchOptions {
                online: self.online,
                yolo: false,
            },
            agent: Some(native.provider.clone()),
            observer_model: Some(self.settings.observer.model.clone()),
            observer_effort: Some(self.settings.observer.effort.clone()),
            jev_model: Some(self.settings.observer.jev_model.clone()),
            ..Default::default()
        };
        for setting in &native.codex_config {
            args.native.extend(["-c".into(), setting.clone()]);
        }
        if native.session.is_none() {
            if let Some(model) = &record.model {
                args.native.extend(["--model".into(), model.clone()]);
            }
            args.effort = record.effort.clone();
        }
        if native.provider == "claude" {
            if native.session.is_none()
                && let Some(name) = &native.name
            {
                args.native.extend(["--name".into(), name.clone()]);
            }
            args.native
                .extend(["--permission-mode".into(), "acceptEdits".into()]);
        }
        let mut command = crate::native_run::command(
            self.home,
            &self.tree,
            Domain::ALL
                .into_iter()
                .find(|d| d.as_str() == record.domain)
                .ok_or("RUN_DOMAIN stage=run_launch")?,
            args,
            if native.session.is_some() {
                native
                    .sending
                    .and_then(|i| record.control.inputs().get(i as usize))
                    .map(|i| i.text.as_str())
                    .or_else(|| native.continuation())
                    .unwrap_or("")
            } else {
                record.control.task()
            },
            native.session.as_deref(),
        )?;
        command.env_remove("CODEX_THREAD_ID");
        command
            .env("KAH_RUN_ID", self.id)
            .env("KAH_RUN_INSTANCE", native.instance.as_deref().unwrap());
        Ok(command)
    }
    fn work(&mut self, child: &mut NativeChild) -> Result<()> {
        loop {
            let pane = self.pane.inspect()?;
            let state = self.synchronize(&pane)?;
            if self.record()?.control.status() != RunStatus::Running {
                return Ok(());
            }
            if child.exited()? {
                self.update(|r| {
                    r.control.stop();
                    Ok(())
                })?;
                return Ok(());
            }
            self.attention(&pane)?;
            self.deliver(&pane)?;
            if let Some(state) = state
                && matches!(pane["agent_status"].as_str(), Some("idle" | "done"))
                && state.inflight.is_none()
            {
                // Stop may precede the durable native completion marker. This
                // existing idle controller loop re-kicks a waiting evidence
                // round; no model polling, extra ask or review deadline.
                if crate::observer_review::gate_approval::pending_followups(&state.binding.session)?
                    && self
                        .history
                        .as_mut()
                        .map(History::completed)
                        .transpose()?
                        .unwrap_or(false)
                    && state.action_digest != self.followup_sample
                {
                    crate::observer_review::hook::spawn("__gate-followup", &state.binding.session)?;
                    self.followup_sample = state.action_digest.clone();
                }
                self.assess(&state)?;
            }
            if self.record()?.control.status() != RunStatus::Running {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }
}
