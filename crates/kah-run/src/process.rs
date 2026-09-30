//! Bounded native JSON intake and responsive cancellation of one exec process.
use kah_codex::exec_events::{Event, decode};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

pub enum Directive {
    Continue,
    Stop(String),
}
pub struct ResultRecord {
    pub success: bool,
    pub stopped: Option<String>,
}
struct Process {
    child: Child,
    messages: Receiver<Result<Event, String>>,
    reaped: bool,
}

pub fn execute(
    mut command: Command,
    prompt: &str,
    log: &Path,
    mut observe: impl FnMut(Option<&Event>) -> Result<Directive, String>,
) -> Result<ResultRecord, String> {
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(log)
        .map_err(|e| format!("RUN_LOG_CREATE stage=worker: {e}"))?;
    let stderr = output.try_clone().map_err(io_error)?;
    let mut child = command
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
        .map_err(io_error)?;
    let stdout = child.stdout.take().unwrap();
    let (sender, messages) = mpsc::sync_channel(16);
    thread::spawn(move || read_events(stdout, output, sender));
    let mut process = Process {
        child,
        messages,
        reaped: false,
    };
    process
        .child
        .stdin
        .take()
        .unwrap()
        .write_all(prompt.as_bytes())
        .map_err(io_error)?;
    let mut usage_seen = false;
    let mut native_failed = false;
    loop {
        let event = match process.messages.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => Some(event?),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                return finish(&mut process, usage_seen, native_failed);
            }
        };
        native_failed |= matches!(&event, Some(Event::Failed(_)));
        if matches!(&event, Some(Event::Usage { .. })) {
            if usage_seen {
                return Err("CODEX_EXEC_DUPLICATE_USAGE stage=worker".into());
            }
            usage_seen = true;
        }
        if let Directive::Stop(reason) = observe(event.as_ref())? {
            process.terminate();
            return Ok(ResultRecord {
                success: false,
                stopped: Some(reason),
            });
        }
    }
}

fn read_events(stdout: impl Read, mut log: File, sender: mpsc::SyncSender<Result<Event, String>>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let event = match reader
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => break,
            Ok(count) if count > 8 * 1024 * 1024 => {
                Err("CODEX_EXEC_LINE_BOUND stage=worker".into())
            }
            Ok(_) => serde_json::from_slice(&line)
                .map_err(|_| "CODEX_EXEC_JSON stage=worker".into())
                .and_then(decode),
            Err(e) => Err(io_error(e)),
        };
        if log.write_all(&line).is_err() {
            let _ = sender.send(Err("RUN_LOG_WRITE stage=worker".into()));
            break;
        }
        let failed = event.is_err();
        if sender.send(event).is_err() || failed {
            break;
        }
    }
}
fn finish(
    process: &mut Process,
    usage_seen: bool,
    native_failed: bool,
) -> Result<ResultRecord, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = process.child.try_wait().map_err(io_error)? {
            process.reaped = true;
            if status.success() && !usage_seen && !native_failed {
                return Err("CODEX_EXEC_USAGE_MISSING stage=worker".into());
            }
            return Ok(ResultRecord {
                success: status.success() && !native_failed,
                stopped: None,
            });
        }
        if Instant::now() >= deadline {
            return Err("CODEX_EXEC_EXIT_TIMEOUT stage=worker".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
impl Process {
    fn terminate(&mut self) {
        if self.reaped {
            return;
        }
        let group = format!("-{}", self.child.id());
        let _ = Command::new("/bin/kill")
            .args(["-TERM", "--", &group])
            .output();
        // Retain the unreaped leader during the grace period so its PID cannot
        // be reused, and still terminate descendants if the leader exited.
        thread::sleep(Duration::from_millis(250));
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &group])
            .output();
        let _ = self.child.wait();
        self.reaped = true;
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.terminate();
    }
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_PROCESS_IO stage=worker: {error}")
}
