//! A per-run file exchange works with gVisor's host sockets/FIFOs disabled.
//! Only bounded regular files are opened; neither endpoint follows symlinks.
use crate::fetch_policy::FetchPolicy;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const BODY_LIMIT: usize = 2 * 1024 * 1024;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    url: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    error: Option<String>,
    bytes: usize,
}

pub struct Broker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Broker {
    pub fn start(directory: PathBuf, policy: FetchPolicy) -> Result<Self, String> {
        if !fs::symlink_metadata(&directory).map_err(io_error)?.is_dir() {
            return Err("FETCH_DIRECTORY stage=fetch_exchange".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let running = stop.clone();
        let worker = thread::spawn(move || {
            while !running.load(Ordering::Relaxed) {
                if let Ok(entries) = fs::read_dir(&directory) {
                    for entry in entries.take(64).flatten() {
                        let name = entry.file_name();
                        let Some(id) = name.to_str().and_then(request_id) else {
                            continue;
                        };
                        if fs::symlink_metadata(directory.join(format!("reply-{id}.json"))).is_ok()
                        {
                            continue;
                        }
                        let _ = serve(&directory, id, &policy);
                        if running.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        Ok(Self {
            stop,
            thread: Some(worker),
        })
    }
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}
fn serve(directory: &Path, id: &str, policy: &FetchPolicy) -> Result<(), String> {
    let result = read_regular(&directory.join(format!("request-{id}.json")), 8192)
        .and_then(|bytes| {
            serde_json::from_slice::<Request>(&bytes)
                .map_err(|_| "FETCH_REQUEST_JSON stage=fetch_exchange".into())
        })
        .and_then(|request| policy.get(&request.url));
    let reply = match result {
        Ok(bytes) => {
            write_atomic(directory, &format!("body-{id}"), &bytes)?;
            Reply {
                error: None,
                bytes: bytes.len(),
            }
        }
        Err(error) => Reply {
            error: Some(error),
            bytes: 0,
        },
    };
    write_atomic(
        directory,
        &format!("reply-{id}.json"),
        &serde_json::to_vec(&reply).unwrap(),
    )
}
pub fn request(directory: &Path, url: &str) -> Result<Vec<u8>, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "FETCH_CLOCK stage=fetch_exchange")?
        .as_nanos();
    let id = format!("{:x}-{nonce:x}", std::process::id());
    write_atomic(
        directory,
        &format!("request-{id}.json"),
        &serde_json::to_vec(&Request { url: url.into() }).unwrap(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let reply_path = directory.join(format!("reply-{id}.json"));
    loop {
        if fs::symlink_metadata(&reply_path).is_ok() {
            let result = read_reply(directory, &id);
            for name in [
                format!("request-{id}.json"),
                format!("reply-{id}.json"),
                format!("body-{id}"),
            ] {
                let _ = fs::remove_file(directory.join(name));
            }
            return result;
        }
        if Instant::now() >= deadline {
            return Err("FETCH_TIMEOUT stage=fetch_exchange".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
fn read_reply(directory: &Path, id: &str) -> Result<Vec<u8>, String> {
    let reply: Reply = serde_json::from_slice(&read_regular(
        &directory.join(format!("reply-{id}.json")),
        8192,
    )?)
    .map_err(|_| "FETCH_REPLY_JSON stage=fetch_exchange")?;
    if let Some(error) = reply.error {
        return Err(error);
    }
    let bytes = read_regular(&directory.join(format!("body-{id}")), BODY_LIMIT)?;
    if bytes.len() != reply.bytes {
        return Err("FETCH_REPLY_SIZE stage=fetch_exchange".into());
    }
    Ok(bytes)
}
pub fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(io_error)?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err("FETCH_FILE_TYPE stage=fetch_exchange".into());
    }
    let mut bytes = vec![];
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err("FETCH_FILE_BOUND stage=fetch_exchange".into());
    }
    Ok(bytes)
}
fn write_atomic(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let temporary = directory.join(format!("{name}.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o660)
        .open(&temporary)
        .map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.set_permissions(fs::Permissions::from_mode(0o660))
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(temporary, directory.join(name)).map_err(io_error)?;
    Ok(())
}
fn request_id(name: &str) -> Option<&str> {
    let id = name.strip_prefix("request-")?.strip_suffix(".json")?;
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
        .then_some(id)
}
fn io_error(error: std::io::Error) -> String {
    format!("FETCH_IO stage=fetch_exchange: {error}")
}
