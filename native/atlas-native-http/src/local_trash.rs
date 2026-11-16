//! Run the portable worker's crash-recoverable cleanup for a local workspace.
use std::{fs, io, os::{fd::AsRawFd, unix::fs::OpenOptionsExt}, path::Path};

pub fn cleanup(dir: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let pending = {
        let _guard = atlas_native_storage_contract::workspace_lock(dir)?;
        let db = rusqlite::Connection::open(dir.join("atlas.sqlite3"))?;
        let cutoff = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs_f64() - 86400.0;
        let expired: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM trash WHERE deleted_at <= ?)", [cutoff], |r| r.get(0))?;
        expired || dir.join(".atlas-purge-pending").exists()
    };
    if !pending { return Ok(false); }

    // Index preparation holds this lock before taking workspace_lock. Use the
    // same order and defer cleanup rather than blocking an active preparation.
    let index = dir.join(".atlas-semantic-native");
    match fs::create_dir(&index) {
        Ok(()) => {},
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {},
        Err(e) => return Err(e.into()),
    }
    let _prepare = match fs::symlink_metadata(&index) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            let lock = fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(index.join(".prepare.lock"))?;
            if !lock.metadata()?.is_file() { return Err("invalid index lock".into()); }
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::WouldBlock { return Ok(false); }
                return Err(e.into());
            }
            Some(lock)
        },
        Ok(_) => return Err("invalid index directory".into()),
        Err(e) => return Err(e.into()),
    };
    let fd = fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW).open(dir)?;
    let mut worker = crate::workspace_worker::Worker::start(&fd)?;
    let response = worker.request(&serde_json::json!({"action":"purge-expired"}))?;
    let value = response.get("ok").ok_or_else(|| response["error"].as_str().unwrap_or("cleanup failed").to_owned())?;
    Ok(value["purged"].as_bool().unwrap_or(false))
}
