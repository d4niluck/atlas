//! Run crash-recoverable cleanup anchored to the already-open workspace.
use std::{ffi::OsStr, fs, io, os::fd::AsRawFd};

pub fn cleanup(dir: &fs::File) -> Result<bool, Box<dyn std::error::Error>> {
    // The descriptor was opened while workspace routing was serialized. Never
    // reopen the directory by name after releasing that serialization.
    let name=OsStr::new(".atlas-semantic-native");
    let index=match crate::direct_export::openat(dir,name,libc::O_RDONLY|libc::O_DIRECTORY,0) {
        Ok(index)=>index,
        Err(error) if error.downcast_ref::<io::Error>().is_some_and(|e|e.kind()==io::ErrorKind::NotFound)=>{
            match crate::direct_export::mkdirat(dir,name) {
                Ok(index)=>index,
                Err(error) if error.downcast_ref::<io::Error>().is_some_and(|e|e.kind()==io::ErrorKind::AlreadyExists)=>crate::direct_export::openat(dir,name,libc::O_RDONLY|libc::O_DIRECTORY,0)?,
                Err(error)=>return Err(error),
            }
        },
        Err(error)=>return Err(error),
    };
    let lock=crate::direct_export::openat(&index,OsStr::new(".prepare.lock"),libc::O_RDWR|libc::O_CREAT,0o600)?;
    if !lock.metadata()?.is_file(){return Err("invalid index lock".into());}
    if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0 {
        let error=io::Error::last_os_error();
        if error.kind()==io::ErrorKind::WouldBlock{return Ok(false)}
        return Err(error.into());
    }
    let mut worker=crate::workspace_worker::Worker::start(dir)?;
    let response=worker.request(&serde_json::json!({"action":"purge-expired"}))?;
    let value=response.get("ok").ok_or_else(||response["error"].as_str().unwrap_or("cleanup failed").to_owned())?;
    Ok(value["purged"].as_bool().unwrap_or(false))
}
