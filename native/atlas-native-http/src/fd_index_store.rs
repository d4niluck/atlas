//! Index generation operations anchored to an opened workspace directory.
use super::*;
use std::{ffi::{CString,OsStr},os::{fd::AsRawFd,unix::{ffi::OsStrExt,fs::MetadataExt}}};

pub(super) struct Store {workspace:fs::File,root:fs::File,name:String}
impl Store {
 pub(super) fn open(name:&str)->R<Self>{
  let workspace=fs::File::open(".")?;
  let root=crate::direct_export::openat(&workspace,OsStr::new(name),libc::O_RDONLY|libc::O_DIRECTORY,0)?;
  #[cfg(feature="test-hook")] if let Some(path)=std::env::var_os("ATLAS_TEST_INDEX_ROOT_PIN_BARRIER"){let dir=Path::new(&path);let _=fs::write(dir.join("ready"),b"ready");let until=std::time::Instant::now()+std::time::Duration::from_secs(10);while !dir.join("go").exists()&&std::time::Instant::now()<until{std::thread::sleep(std::time::Duration::from_millis(5));}}
  Ok(Self{workspace,root,name:name.to_owned()})
 }
 pub(super) fn verify_root(&self)->R<()> {
  let current=crate::direct_export::openat(&self.workspace,OsStr::new(&self.name),libc::O_RDONLY|libc::O_DIRECTORY,0)?;
  let a=self.root.metadata()?;let b=current.metadata()?;
  if (a.dev(),a.ino())!=(b.dev(),b.ino()){return Err("index directory changed during cleanup".into())}Ok(())
 }
 pub(super) fn open_gen(&self,name:&str)->R<fs::File>{if !valid_worker_generation(name){return Err("invalid index generation".into())}crate::direct_export::openat(&self.root,OsStr::new(name),libc::O_RDONLY|libc::O_DIRECTORY,0)}
 pub(super) fn verify_gen(&self,name:&str,expected:&fs::File)->R<()> {let current=self.open_gen(name)?;let a=expected.metadata()?;let b=current.metadata()?;if (a.dev(),a.ino())!=(b.dev(),b.ino()){return Err("index generation changed during cleanup".into())}Ok(())}
 pub(super) fn create_gen(&self,name:&str)->R<fs::File>{if !valid_worker_generation(name){return Err("invalid index generation".into())}self.verify_root()?;crate::direct_export::mkdirat(&self.root,OsStr::new(name))}
 pub(super) fn read(&self,dir:&fs::File,name:&str,limit:u64)->R<Vec<u8>> {let mut file=crate::direct_export::openat(dir,OsStr::new(name),libc::O_RDONLY|libc::O_NONBLOCK,0)?;if !file.metadata()?.is_file()||file.metadata()?.len()>limit{return Err("invalid index file".into())}let mut data=Vec::new();file.read_to_end(&mut data)?;Ok(data)}
 pub(super) fn write(&self,dir:&fs::File,name:&str,bytes:&[u8])->R<()> {let mut file=crate::direct_export::openat(dir,OsStr::new(name),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL,0o600)?;file.write_all(bytes)?;file.sync_all()?;Ok(())}
 pub(super) fn write_index(&self,dir:&fs::File,index:&atlas_faiss_probe::Index)->R<String>{let mut file=crate::direct_export::openat(dir,OsStr::new("index.faiss"),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL,0o600)?;index.save_file(&mut file)?;file.sync_all()?;file_hash(&mut file)}
 pub(super) fn copy_file(&self,source:&fs::File,dest:&fs::File,name:&str)->R<()> {let mut input=crate::direct_export::openat(source,OsStr::new(name),libc::O_RDONLY|libc::O_NONBLOCK,0)?;let mut output=crate::direct_export::openat(dest,OsStr::new(name),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL,0o600)?;io::copy(&mut input,&mut output)?;output.sync_all()?;if file_hash(&mut input)?!=file_hash(&mut output)?{return Err("index copy verification failed".into())}Ok(())}
 pub(super) fn current(&self)->R<String>{Ok(String::from_utf8(self.read(&self.root,"current",256)?)?)}
 pub(super) fn publish(&self,name:&str,previous:&str)->R<()> {
  if !valid_worker_generation(name){return Err("invalid index generation".into())}self.verify_root()?;if self.current()?!=previous{return Err("index changed during cleanup".into())}
  let temp=format!(".current-{}",uuid::Uuid::new_v4().simple());self.write(&self.root,&temp,name.as_bytes())?;
  let from=CString::new(temp.as_bytes())?;let to=CString::new("current")?;
  self.verify_root()?;if unsafe{libc::renameat(self.root.as_raw_fd(),from.as_ptr(),self.root.as_raw_fd(),to.as_ptr())}!=0{return Err(io::Error::last_os_error().into())}self.root.sync_all()?;Ok(())
 }
 pub(super) fn cleanup(&self,current:&str,base:Option<&str>)->R<()> {self.verify_root()?;if self.current()?!=current{return Err("index changed during cleanup".into())}for entry in crate::direct_export::entries(&self.root)?{let name=entry.to_string_lossy();if name!=current&&base!=Some(name.as_ref())&&valid_worker_generation(&name){if self.current()?!=current{return Err("index changed during cleanup".into())}self.remove_gen(&name)?;}}self.root.sync_all()?;Ok(())}
 pub(super) fn remove_gen(&self,name:&str)->R<()> {if !valid_worker_generation(name){return Err("invalid index generation".into())}self.verify_root()?;remove_dir(&self.root,OsStr::new(name))}
}
pub(super) fn unlink(dir:&fs::File,name:&OsStr,flags:i32)->R<()> {let name=CString::new(name.as_bytes())?;if unsafe{libc::unlinkat(dir.as_raw_fd(),name.as_ptr(),flags)}!=0{return Err(io::Error::last_os_error().into())}Ok(())}
pub(super) fn remove_dir(parent:&fs::File,name:&OsStr)->R<()> {
 let dir=crate::direct_export::openat(parent,name,libc::O_RDONLY|libc::O_DIRECTORY,0)?;
 for child in crate::direct_export::entries(&dir)?{
  match crate::direct_export::openat(&dir,&child,libc::O_RDONLY|libc::O_DIRECTORY,0){Ok(_)=>remove_dir(&dir,&child)?,Err(e) if e.downcast_ref::<io::Error>().is_some_and(|e|matches!(e.raw_os_error(),Some(libc::ENOTDIR)|Some(libc::ELOOP)))=>unlink(&dir,&child,0)?,Err(e)=>return Err(e)}
 }
 unlink(parent,name,libc::AT_REMOVEDIR)
}
