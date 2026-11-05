//! Fail-closed marker check relative to the already selected directory.
use std::{fs::File,os::fd::AsRawFd};

pub fn is_protected_dir(directory:&File)->Result<bool,String>{
 let metadata=directory.metadata().map_err(|_|"Не удалось проверить папку".to_owned())?;
 if !metadata.is_dir(){return Err("Выбрана не папка".into())}
 for name in [c".atlas-protected",c"workspace.atlas"] {
  let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();
  if unsafe{libc::fstatat(directory.as_raw_fd(),name.as_ptr(),stat.as_mut_ptr(),libc::AT_SYMLINK_NOFOLLOW)}==0{return Ok(true)}
  if std::io::Error::last_os_error().raw_os_error()!=Some(libc::ENOENT){return Err("Не удалось проверить папку".into())}
 }
 Ok(false)
}
