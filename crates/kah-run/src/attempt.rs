//! One native attempt, with short state transactions between streamed events.
use crate::{
    process::{self, Directive},
    store::Store,
};
use kah_codex::exec_events::Event;
use kah_core::unattended::RunStatus;
use serde_json::Value;
use std::{
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub struct Outcome {
    pub success: bool,
    pub stopped: Option<String>,
    pub thread: Option<String>,
    pub actions: Vec<Value>,
}
struct Monitor {
    sequence: u64,
    last: Instant,
    thread: Option<String>,
    actions: Vec<Value>,
}
pub fn run(
    base: &Path,
    id: &str,
    command: Command,
    prompt: &str,
    log: &Path,
) -> Result<Outcome, String> {
    let record = Store::read(base, id)?;
    if record.control.status() != RunStatus::Running {
        return Err("RUN_NOT_RUNNING stage=attempt".into());
    }
    let mut monitor = Monitor {
        sequence: record.control.inputs().last().unwrap().sequence,
        last: Instant::now(),
        thread: None,
        actions: vec![],
    };
    let result = process::execute(command, prompt, log, |event| monitor.tick(base, id, event))?;
    let mut store = locked(base, id)?;
    let mut record = store.load()?;
    record
        .control
        .account(0, 0, monitor.last.elapsed().as_millis() as u64);
    if result.stopped.as_deref() == Some("RUN_USER_STOP") {
        record.control.stop();
    }
    store.save(&record)?;
    Ok(Outcome {
        success: result.success,
        stopped: result.stopped,
        thread: monitor.thread,
        actions: monitor.actions,
    })
}
impl Monitor {
    fn tick(&mut self, base: &Path, id: &str, event: Option<&Event>) -> Result<Directive, String> {
        self.capture(event);
        let mut store = locked(base, id)?;
        let mut record = store.load()?;
        let elapsed = self.last.elapsed().as_millis() as u64;
        let (input, output) = match event {
            Some(Event::Usage { input, output, .. }) => (*input, *output),
            _ => (0, 0),
        };
        let status = record.control.status();
        let changed = record.control.inputs().last().unwrap().sequence != self.sequence;
        if elapsed >= 250 || input != 0 || output != 0 || status != RunStatus::Running || changed {
            record.control.account(input, output, elapsed);
            self.last = Instant::now();
            store.save(&record)?;
        }
        let reason = match record.control.status() {
            RunStatus::Stopping | RunStatus::Stopped => Some("RUN_USER_STOP"),
            RunStatus::BudgetStopped => Some("RUN_TOKEN_BUDGET"),
            RunStatus::TimeStopped => Some("RUN_TIME_LIMIT"),
            RunStatus::Running if changed => Some("RUN_STEER"),
            RunStatus::Running => None,
            _ => Some("RUN_STATE_CHANGED"),
        };
        Ok(reason.map_or(Directive::Continue, |code| Directive::Stop(code.into())))
    }
    fn capture(&mut self, event: Option<&Event>) {
        match event {
            Some(Event::ThreadStarted(id)) => self.thread = Some(id.clone()),
            Some(Event::Action(value)) => {
                let mut value = value.clone();
                value["id"] = format!(
                    "{}:{}",
                    self.thread.as_deref().unwrap_or("pending"),
                    value["id"].as_str().unwrap_or("unknown")
                )
                .into();
                self.actions.push(value);
                while self.actions.len() > 12
                    || serde_json::to_vec(&self.actions).unwrap().len() > 48 * 1024
                {
                    self.actions.remove(0);
                }
            }
            _ => (),
        }
    }
}
fn locked(base: &Path, id: &str) -> Result<Store, String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match Store::open(base, id) {
            Err(error) if error.starts_with("RUN_BUSY") && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20))
            }
            result => return result,
        }
    }
}
