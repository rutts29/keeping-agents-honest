use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};
pub struct Receipt {
    directory: PathBuf,
    digest: String,
    pub state: String,
    _lock: File,
}
impl Receipt {
    pub fn open(base: &Path, message: &Message) -> Result<Self, String> {
        let id = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{}", message.session, message.client_id).as_bytes())
        );
        let directory = base.join("delegate-deliveries").join(id);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(io_error)?;
        let meta = fs::symlink_metadata(&directory).map_err(io_error)?;
        if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
            return Err("SEND_PRIVATE_STATE stage=delegate_channel".into());
        }
        let lock = open_private(&directory.join("lock"))?;
        lock.try_lock()
            .map_err(|_| "SEND_RECEIPT_BUSY stage=delegate_channel")?;
        let path = directory.join("receipt.json");
        check(&path)?;
        let digest = message.digest();
        let state = match fs::read(&path) {
            Ok(bytes) => {
                let v: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| "SEND_RECEIPT_JSON stage=delegate_channel")?;
                if v["payloadDigest"] != digest {
                    return Err("SEND_RECEIPT_COLLISION stage=delegate_channel".into());
                }
                v["state"]
                    .as_str()
                    .ok_or("SEND_RECEIPT_STATE stage=delegate_channel")?
                    .into()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "new".into(),
            Err(e) => return Err(io_error(e)),
        };
        Ok(Self {
            directory,
            digest,
            state,
            _lock: lock,
        })
    }
    /// Read a durable acknowledgement without creating or changing a receipt.
    pub fn accepted(base: &Path, message: &Message) -> Result<bool, String> {
        let id = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{}", message.session, message.client_id).as_bytes())
        );
        let directory = base.join("delegate-deliveries").join(id);
        match fs::symlink_metadata(&directory) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(io_error(e)),
            Ok(meta) if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 => {
                return Err("SEND_PRIVATE_STATE stage=delegate_channel".into());
            }
            _ => (),
        }
        let path = directory.join("receipt.json");
        check(&path)?;
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(io_error(e)),
        };
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "SEND_RECEIPT_JSON stage=delegate_channel")?;
        if value["payloadDigest"] != message.digest() {
            return Err("SEND_RECEIPT_COLLISION stage=delegate_channel".into());
        }
        Ok(value["state"] == "accepted")
    }
    pub fn save(&mut self, state: &str) -> Result<(), String> {
        let temporary = self.directory.join("receipt.tmp");
        let mut file = open_private(&temporary)?;
        file.set_len(0).map_err(io_error)?;
        file.write_all(
            json!({"payloadDigest":self.digest,"state":state})
                .to_string()
                .as_bytes(),
        )
        .map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(temporary, self.directory.join("receipt.json")).map_err(io_error)?;
        File::open(&self.directory)
            .and_then(|f| f.sync_all())
            .map_err(io_error)?;
        self.state = state.into();
        Ok(())
    }
}
fn check(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.permissions().mode() & 0o077 != 0)
    {
        return Err("SEND_PRIVATE_FILE stage=delegate_channel".into());
    }
    Ok(())
}
fn open_private(path: &Path) -> Result<File, String> {
    check(path)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(io_error)
}
fn io_error(e: std::io::Error) -> String {
    format!("SEND_IO stage=delegate_channel: {e}")
}
