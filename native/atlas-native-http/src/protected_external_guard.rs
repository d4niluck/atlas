//! Keeps the selected directory and protected-workspace lock pinned together.
use std::{io::{Read,Seek,SeekFrom},fs::{File,OpenOptions},os::{fd::{AsRawFd,FromRawFd},unix::fs::{MetadataExt,OpenOptionsExt}},path::{Path,PathBuf}};
pub struct ProtectedExternalGuard { directory:File, lock:File, database:File, path:PathBuf, device:u64, inode:u64 }
fn open_regular_at(directory:&File,name:&str)->Result<File,String>{let name=std::ffi::CString::new(name).map_err(|_|"Неверный файл")?;let fd=unsafe{libc::openat(directory.as_raw_fd(),name.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK)};if fd<0{return Err("Файл пространства недоступен".into())}let file=unsafe{File::from_raw_fd(fd)};if !file.metadata().map_err(|_|"Файл пространства недоступен")?.is_file(){return Err("Неверный файл пространства".into())}Ok(file)}
fn verify_sidecars(directory:&File)->Result<(),String>{
 for name in ["workspace.atlas-journal","workspace.atlas-wal","workspace.atlas-shm"] {
  let name=std::ffi::CString::new(name).unwrap();let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();let result=unsafe{libc::fstatat(directory.as_raw_fd(),name.as_ptr(),stat.as_mut_ptr(),libc::AT_SYMLINK_NOFOLLOW)};
  if result!=0 {if std::io::Error::last_os_error().kind()==std::io::ErrorKind::NotFound{continue}return Err("Не удалось проверить служебный файл".into())}
  let stat=unsafe{stat.assume_init()};if stat.st_mode & libc::S_IFMT != libc::S_IFREG{return Err("Недопустимый служебный файл".into())}
 }Ok(())
}
fn same_file(a:&File,b:&File)->Result<bool,String>{let a=a.metadata().map_err(|_|"Файл пространства недоступен")?;let b=b.metadata().map_err(|_|"Файл пространства недоступен")?;Ok((a.dev(),a.ino())==(b.dev(),b.ino()))}
impl ProtectedExternalGuard {
 pub fn acquire(directory:File,path:&Path)->Result<Self,String>{
  let metadata=directory.metadata().map_err(|_|"Носитель недоступен")?;
  if !metadata.is_dir(){return Err("Неверная папка пространства".into())}
  let path_meta=std::fs::symlink_metadata(path).map_err(|_|"Носитель недоступен")?;
  if path_meta.file_type().is_symlink()||(metadata.dev(),metadata.ino())!=(path_meta.dev(),path_meta.ino()){return Err("Папка пространства изменилась".into())}
  let name=std::ffi::CString::new(".atlas-protected").unwrap();
  let fd=unsafe{libc::openat(directory.as_raw_fd(),name.as_ptr(),libc::O_RDWR|libc::O_NOFOLLOW|libc::O_CLOEXEC|libc::O_NONBLOCK)};
  if fd<0{return Err("Защищённое пространство недоступно".into())}
  let mut lock=unsafe{File::from_raw_fd(fd)};
  if !lock.metadata().map_err(|_|"Пространство недоступно")?.is_file(){return Err("Неверный маркер пространства".into())}
  let mut marker=Vec::new();lock.by_ref().take(129).read_to_end(&mut marker).map_err(|_|"Неверный маркер пространства")?;if marker!=b"atlas-protected-v1"{return Err("Неверный маркер пространства".into())}lock.seek(SeekFrom::Start(0)).map_err(|_|"Неверный маркер пространства")?;
  if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Пространство уже открыто".into())}
  let database=open_regular_at(&directory,"workspace.atlas")?;
  let guard=Self{directory,lock,database,path:path.to_owned(),device:metadata.dev(),inode:metadata.ino()};guard.verify()?;Ok(guard)
 }
 pub fn verify(&self)->Result<(),String>{
  let current=OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(&self.path).map_err(|_|"Носитель отключён")?;
  let meta=current.metadata().map_err(|_|"Носитель отключён")?;
  let pinned=self.directory.metadata().map_err(|_|"Носитель отключён")?;
  if (meta.dev(),meta.ino())!=(self.device,self.inode)||(pinned.dev(),pinned.ino())!=(self.device,self.inode){return Err("Папка пространства изменилась".into())}
  if !same_file(&self.lock,&open_regular_at(&self.directory,".atlas-protected")?)?||!same_file(&self.database,&open_regular_at(&self.directory,"workspace.atlas")?)?{return Err("Файл пространства изменился".into())}verify_sidecars(&self.directory)?;Ok(())
 }
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn lock_and_detect_replacement(){
  let root=std::env::temp_dir().join(format!("atlas-guard-{}",uuid::Uuid::new_v4()));
  std::fs::create_dir(&root).unwrap();let path=root.join("space");std::fs::create_dir(&path).unwrap();std::fs::write(path.join(".atlas-protected"),b"atlas-protected-v1").unwrap();std::fs::write(path.join("workspace.atlas"),b"fixture").unwrap();
  let first=ProtectedExternalGuard::acquire(File::open(&path).unwrap(),&path).unwrap();assert!(first.verify().is_ok());
  std::os::unix::fs::symlink("/tmp",path.join("workspace.atlas-journal")).unwrap();assert!(first.verify().is_err());std::fs::remove_file(path.join("workspace.atlas-journal")).unwrap();assert!(first.verify().is_ok());assert!(ProtectedExternalGuard::acquire(File::open(&path).unwrap(),&path).is_err());
  std::fs::rename(path.join("workspace.atlas"),path.join("old.atlas")).unwrap();std::fs::write(path.join("workspace.atlas"),b"replacement").unwrap();assert!(first.verify().is_err());std::fs::remove_file(path.join("workspace.atlas")).unwrap();std::fs::rename(path.join("old.atlas"),path.join("workspace.atlas")).unwrap();assert!(first.verify().is_ok());
  std::fs::rename(path.join(".atlas-protected"),path.join("old.marker")).unwrap();std::fs::write(path.join(".atlas-protected"),b"atlas-protected-v1").unwrap();assert!(first.verify().is_err());std::fs::remove_file(path.join(".atlas-protected")).unwrap();std::fs::rename(path.join("old.marker"),path.join(".atlas-protected")).unwrap();
  std::fs::rename(&path,root.join("original")).unwrap();std::fs::create_dir(&path).unwrap();assert!(first.verify().is_err());drop(first);std::fs::remove_dir_all(root).unwrap();
 }
}
