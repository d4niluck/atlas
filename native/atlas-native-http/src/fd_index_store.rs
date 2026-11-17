//! Index generation operations anchored to an opened workspace directory.
use super::*;
use std::{ffi::{CString,OsStr},os::{fd::AsRawFd,unix::{ffi::OsStrExt,fs::MetadataExt}}};

// Stages are generated from UUID::simple, optionally followed by -base.
fn valid_stage(name:&str)->bool {
 let Some(rest)=name.strip_prefix(".stage-") else{return false};
 let id=rest.strip_suffix("-base").unwrap_or(rest);
 id.len()==32 && id.bytes().all(|c|c.is_ascii_hexdigit())
}

fn valid_retired(name:&str)->bool {
 name.strip_prefix(".retired-").is_some_and(|id|id.len()==32&&id.bytes().all(|c|c.is_ascii_hexdigit()))
}

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
 // Exclusive per-index lease survives process crashes via the OS lock. Every
 // staging writer holds it until publication, so only abandoned stages are removed.
 pub(super) fn staging_guard(&self)->R<fs::File>{
  self.verify_root()?;
  let lock=crate::direct_export::openat(&self.root,OsStr::new(".staging.lock"),libc::O_RDWR|libc::O_CREAT|libc::O_CLOEXEC,0o600)?;
  if !lock.metadata()?.is_file(){return Err("invalid staging lock".into())}
  if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err(io::Error::last_os_error().into())}
  self.verify_root()?;
  for entry in crate::direct_export::entries(&self.root)?{
   if let Some(name)=entry.to_str(){if valid_stage(name){self.remove_stage(name)?}else if valid_retired(name){self.remove_retired(name)?}}
  }
  Ok(lock)
 }
 pub(super) fn create_stage(&self,name:&str)->R<fs::File>{
  if !valid_stage(name){return Err("invalid stage".into())}self.verify_root()?;
  let dir=crate::direct_export::mkdirat(&self.root,OsStr::new(name))?;
  self.write(&dir,".atlas-stage-owner",b"atlas-index-stage-v1")?;dir.sync_all()?;self.root.sync_all()?;Ok(dir)
 }
 // Caller holds the workspace lock; do not overwrite an existing generation.
 pub(super) fn promote_stage(&self,stage:&str,name:&str,expected:&fs::File)->R<()> {
  if !valid_stage(stage)||!valid_worker_generation(name){return Err("invalid stage".into())}
  self.verify_root()?;
  let staged=crate::direct_export::openat(&self.root,OsStr::new(stage),libc::O_RDONLY|libc::O_DIRECTORY,0)?;
  let a=staged.metadata()?;let b=expected.metadata()?;
  if (a.dev(),a.ino())!=(b.dev(),b.ino()){return Err("staged generation changed".into())}
  let from=CString::new(stage.as_bytes())?;let to=CString::new(name.as_bytes())?;
  #[cfg(target_os="macos")] let rc=unsafe{libc::renameatx_np(self.root.as_raw_fd(),from.as_ptr(),self.root.as_raw_fd(),to.as_ptr(),libc::RENAME_EXCL)};
  #[cfg(target_os="linux")] let rc=unsafe{libc::renameat2(self.root.as_raw_fd(),from.as_ptr(),self.root.as_raw_fd(),to.as_ptr(),libc::RENAME_NOREPLACE)};
  if rc!=0{return Err(io::Error::last_os_error().into())}
  self.verify_root()?;self.verify_gen(name,expected)?;self.root.sync_all()?;Ok(())
 }
 pub(super) fn remove_stage(&self,name:&str)->R<()> {
  if !valid_stage(name){return Err("invalid stage".into())}self.verify_root()?;
  let dir=match crate::direct_export::openat(&self.root,OsStr::new(name),libc::O_RDONLY|libc::O_DIRECTORY,0){Ok(dir)=>dir,Err(e) if e.downcast_ref::<io::Error>().is_some_and(|v|matches!(v.raw_os_error(),Some(libc::ENOENT)|Some(libc::ENOTDIR)|Some(libc::ELOOP)))=>return Ok(()),Err(e)=>return Err(e)};
  let owner=match self.read(&dir,".atlas-stage-owner",64){Ok(owner)=>owner,Err(e) if e.downcast_ref::<io::Error>().is_some_and(|v|matches!(v.raw_os_error(),Some(libc::ENOENT)|Some(libc::ENOTDIR)|Some(libc::ELOOP)))=>return Ok(()),Err(e)=>return Err(e)};
  if owner!=b"atlas-index-stage-v1"{return Ok(())}
  remove_dir(&self.root,OsStr::new(name))
 }
 // Caller holds workspace lock: retiring changes only directory entries, not
 // the current pointer. Recursive removal happens after that lock is released.
 fn prepare_retired(&self)->R<(String,fs::File)> {
  let retired_name=format!(".retired-{}",uuid::Uuid::new_v4().simple());
  let dir=crate::direct_export::mkdirat(&self.root,OsStr::new(&retired_name))?;
  self.write(&dir,".owner",b"atlas-retired-generation-v1")?;dir.sync_all()?;self.root.sync_all()?;Ok((retired_name,dir))
 }
 pub(super) fn retire_old(&self,current:&str,base:Option<&str>,dir:&fs::File)->R<bool> {
  const MAX_GENERATIONS:usize=8;
  self.verify_root()?;if self.current()?!=current{return Err("index changed during cleanup".into())}
  let candidates:Vec<_>=crate::direct_export::entries(&self.root)?.into_iter().filter(|entry|entry.to_str().is_some_and(|name|name!=current&&base!=Some(name)&&valid_worker_generation(name))).take(MAX_GENERATIONS).collect();
  if candidates.is_empty(){return Ok(false)}
  for entry in candidates {
   let name=entry.to_str().ok_or("invalid generation")?;let source=self.open_gen(name)?;
   let from=CString::new(name.as_bytes())?;let to=CString::new(name.as_bytes())?;
   #[cfg(target_os="macos")] let rc=unsafe{libc::renameatx_np(self.root.as_raw_fd(),from.as_ptr(),dir.as_raw_fd(),to.as_ptr(),libc::RENAME_EXCL)};
   #[cfg(target_os="linux")] let rc=unsafe{libc::renameat2(self.root.as_raw_fd(),from.as_ptr(),dir.as_raw_fd(),to.as_ptr(),libc::RENAME_NOREPLACE)};
   if rc!=0{return Err(io::Error::last_os_error().into())}
   let moved=crate::direct_export::openat(&dir,OsStr::new(name),libc::O_RDONLY|libc::O_DIRECTORY,0)?;
   let a=source.metadata()?;let b=moved.metadata()?;if (a.dev(),a.ino())!=(b.dev(),b.ino()){return Err("retired generation changed".into())}
   #[cfg(feature="test-hook")] if let Some(barrier)=std::env::var_os("ATLAS_TEST_RETIRE_RENAME_BARRIER") {
    let dir=Path::new(&barrier);fs::write(dir.join("ready"),b"ready")?;let started=std::time::Instant::now();
    while !dir.join("go").exists()&&started.elapsed()<std::time::Duration::from_secs(30){std::thread::sleep(std::time::Duration::from_millis(10));}
   }
  }
  Ok(true)
 }
 pub(super) fn cleanup_batched<G>(&self,current:&str,base:Option<&str>,mut guard:impl FnMut()->R<G>)->R<()> {
  loop {
   // Make the ownership marker durable before taking the workspace lock.
   let (name,dir)=self.prepare_retired()?;
   let retired={let _guard=match guard(){Ok(guard)=>guard,Err(error)=>{drop(dir);self.remove_retired(&name)?;return Err(error)}};self.retire_old(current,base,&dir)};
   // Both fsyncs and recursive removal can be slow on removable media.
   dir.sync_all()?;self.root.sync_all()?;drop(dir);
   match retired {
    Ok(true)=>{
     #[cfg(feature="test-hook")] if let Some(barrier)=std::env::var_os("ATLAS_TEST_RETIRED_DELETE_BARRIER") {
      let dir=Path::new(&barrier);if dir.join("enabled").exists(){fs::write(dir.join("ready"),b"ready")?;let started=std::time::Instant::now();
      while !dir.join("go").exists()&&started.elapsed()<std::time::Duration::from_secs(30){std::thread::sleep(std::time::Duration::from_millis(10));}}
     }
     self.remove_retired(&name)?
    },Ok(false)=>{self.remove_retired(&name)?;return Ok(())},
    Err(error)=>{self.remove_retired(&name)?;return Err(error)}
   }
  }
 }
 pub(super) fn remove_retired(&self,name:&str)->R<()> {
  if !valid_retired(name){return Err("invalid retired index".into())}self.verify_root()?;
  let dir=match crate::direct_export::openat(&self.root,OsStr::new(name),libc::O_RDONLY|libc::O_DIRECTORY,0){Ok(dir)=>dir,Err(e) if e.downcast_ref::<io::Error>().is_some_and(|v|matches!(v.raw_os_error(),Some(libc::ENOENT)|Some(libc::ENOTDIR)|Some(libc::ELOOP)))=>return Ok(()),Err(e)=>return Err(e)};
  let owner=match self.read(&dir,".owner",64){Ok(owner)=>owner,Err(e) if e.downcast_ref::<io::Error>().is_some_and(|v|matches!(v.raw_os_error(),Some(libc::ENOENT)|Some(libc::ENOTDIR)|Some(libc::ELOOP)))=>return Ok(()),Err(e)=>return Err(e)};
  if owner!=b"atlas-retired-generation-v1"{return Ok(())}
  for entry in crate::direct_export::entries(&dir)? {
   if entry!=OsStr::new(".owner")&&entry!=OsStr::new("generation")&&!entry.to_str().is_some_and(valid_worker_generation){return Ok(())}
  }
  remove_dir(&self.root,OsStr::new(name))?;self.root.sync_all()?;Ok(())
 }
 pub(super) fn read(&self,dir:&fs::File,name:&str,limit:u64)->R<Vec<u8>> {let mut file=crate::direct_export::openat(dir,OsStr::new(name),libc::O_RDONLY|libc::O_NONBLOCK,0)?;if !file.metadata()?.is_file()||file.metadata()?.len()>limit{return Err("invalid index file".into())}let mut data=Vec::new();file.read_to_end(&mut data)?;Ok(data)}
 pub(super) fn write(&self,dir:&fs::File,name:&str,bytes:&[u8])->R<()> {let mut file=crate::direct_export::openat(dir,OsStr::new(name),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL,0o600)?;file.write_all(bytes)?;file.sync_all()?;Ok(())}
 pub(super) fn write_index(&self,dir:&fs::File,index:&atlas_faiss_probe::Index)->R<String>{let mut file=crate::direct_export::openat(dir,OsStr::new("index.faiss"),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL,0o600)?;index.save_file(&mut file)?;file.sync_all()?;file_hash(&mut file)}
 pub(super) fn copy_file(&self,source:&fs::File,dest:&fs::File,name:&str)->R<()> {let mut input=crate::direct_export::openat(source,OsStr::new(name),libc::O_RDONLY|libc::O_NONBLOCK,0)?;let mut output=crate::direct_export::openat(dest,OsStr::new(name),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL,0o600)?;io::copy(&mut input,&mut output)?;output.sync_all()?;if file_hash(&mut input)?!=file_hash(&mut output)?{return Err("index copy verification failed".into())}Ok(())}
 pub(super) fn current(&self)->R<String>{Ok(String::from_utf8(self.read(&self.root,"current",256)?)?)}
 pub(super) fn publish_initial(&self,name:&str)->R<()> {
  if !valid_worker_generation(name){return Err("invalid index generation".into())}self.verify_root()?;
  match self.current(){Err(e) if e.downcast_ref::<io::Error>().is_some_and(|e|e.kind()==io::ErrorKind::NotFound)=>{},Err(e)=>return Err(e),Ok(_)=>return Err("index changed during preparation".into())}
  let temp=format!(".current-{}",uuid::Uuid::new_v4().simple());self.write(&self.root,&temp,name.as_bytes())?;
  let from=CString::new(temp.as_bytes())?;let to=CString::new("current")?;
  #[cfg(target_os="macos")] let rc=unsafe{libc::renameatx_np(self.root.as_raw_fd(),from.as_ptr(),self.root.as_raw_fd(),to.as_ptr(),libc::RENAME_EXCL)};
  #[cfg(target_os="linux")] let rc=unsafe{libc::renameat2(self.root.as_raw_fd(),from.as_ptr(),self.root.as_raw_fd(),to.as_ptr(),libc::RENAME_NOREPLACE)};
  if rc!=0{let _=unlink(&self.root,OsStr::new(&temp),0);return Err(io::Error::last_os_error().into())}self.root.sync_all()?;Ok(())
 }
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

#[cfg(test)]
mod staging_tests {
 use super::*;
 #[test]
 fn abandoned_stages_are_cleaned_only_without_active_lease() {
  let path=std::env::temp_dir().join(format!("atlas-stage-lease-{}",uuid::Uuid::new_v4().simple()));
  fs::create_dir(&path).unwrap();fs::create_dir(path.join("index")).unwrap();
  let make_store=||Store{workspace:fs::File::open(&path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
  let first=make_store();let second=make_store();
  let lease=first.staging_guard().unwrap();
  let stage=format!(".stage-{}",uuid::Uuid::new_v4().simple());
  fs::create_dir(path.join("index/.stage-unrelated")).unwrap();
  let foreign_stage=format!(".stage-{}",uuid::Uuid::new_v4().simple());fs::create_dir(path.join("index").join(&foreign_stage)).unwrap();
  let foreign_retired=format!(".retired-{}",uuid::Uuid::new_v4().simple());fs::create_dir(path.join("index").join(&foreign_retired)).unwrap();
  let dir=first.create_stage(&stage).unwrap();drop(dir);
  assert!(second.staging_guard().is_err());
  assert!(path.join("index").join(&stage).exists(),"active stage was removed");
  drop(lease);
  let recovered=second.staging_guard().unwrap();
  assert!(!path.join("index").join(&stage).exists(),"abandoned stage remains");
  assert!(path.join("index/.stage-unrelated").exists(),"unrelated directory was removed");
  assert!(path.join("index").join(&foreign_stage).exists(),"unowned stage was removed");
  assert!(path.join("index").join(&foreign_retired).exists(),"unowned retired directory was removed");
  drop(recovered);drop(first);drop(second);fs::remove_dir_all(path).unwrap();
 }
 #[test]
 fn abandoned_stage_is_recovered_after_process_is_killed() {
  use std::io::BufRead;
  let path=std::env::temp_dir().join(format!("atlas-stage-crash-{}",uuid::Uuid::new_v4().simple()));
  fs::create_dir(&path).unwrap();fs::create_dir(path.join("index")).unwrap();
  let store=Store{workspace:fs::File::open(&path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
  let stage=format!(".stage-{}",uuid::Uuid::new_v4().simple());drop(store.create_stage(&stage).unwrap());
  let mut child=std::process::Command::new("python3").arg("-u").arg("-c")
   .arg("import fcntl,sys,time; f=open(sys.argv[1], 'a+'); fcntl.flock(f, fcntl.LOCK_EX); print('ready', flush=True); time.sleep(60)")
   .arg(path.join("index/.staging.lock")).stdout(std::process::Stdio::piped()).spawn().unwrap();
  let mut line=String::new();std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();assert_eq!(line.trim(),"ready");
  assert!(store.staging_guard().is_err());assert!(path.join("index").join(&stage).exists());
  child.kill().unwrap();child.wait().unwrap();
  let guard=store.staging_guard().unwrap();assert!(!path.join("index").join(&stage).exists());
  drop(guard);drop(store);fs::remove_dir_all(path).unwrap();
 }
 #[test]
 fn retired_generations_recover_after_interruption_without_touching_current_base_or_foreign_data() {
  let path=std::env::temp_dir().join(format!("atlas-retired-{}",uuid::Uuid::new_v4().simple()));
  fs::create_dir(&path).unwrap();fs::create_dir(path.join("index")).unwrap();
  let store=Store{workspace:fs::File::open(&path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
  let current=format!("gen-{}",uuid::Uuid::new_v4().simple());let base=format!("gen-{}",uuid::Uuid::new_v4().simple());let old=format!("gen-{}",uuid::Uuid::new_v4().simple());
  drop(store.create_gen(&current).unwrap());drop(store.create_gen(&base).unwrap());let old_dir=store.create_gen(&old).unwrap();store.write(&old_dir,"index.faiss",b"obsolete").unwrap();drop(old_dir);
  fs::write(path.join("index/current"),current.as_bytes()).unwrap();
  let lease=store.staging_guard().unwrap();let (retired,dir)=store.prepare_retired().unwrap();assert!(store.retire_old(&current,Some(&base),&dir).unwrap());drop(dir);
  assert!(path.join("index").join(&current).exists());assert!(path.join("index").join(&base).exists());assert!(!path.join("index").join(&old).exists());
  assert!(path.join("index").join(&retired).exists());drop(lease);
  let next=store.staging_guard().unwrap();assert!(!path.join("index").join(&retired).exists());assert!(path.join("index").join(&current).exists());assert!(path.join("index").join(&base).exists());
  drop(next);drop(store);fs::remove_dir_all(path).unwrap();
 }
 #[cfg(feature="test-hook")]
 #[test]
 fn interrupted_retire_after_rename_recovers_without_removing_current() {
  if let Some(raw)=std::env::var_os("ATLAS_TEST_RETIRE_CHILD") {
   let path=Path::new(&raw);let store=Store{workspace:fs::File::open(path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
   let current=fs::read_to_string(path.join("index/current")).unwrap();let _lease=store.staging_guard().unwrap();let (_name,dir)=store.prepare_retired().unwrap();store.retire_old(&current,None,&dir).unwrap();return
  }
  let path=std::env::temp_dir().join(format!("atlas-retire-crash-{}",uuid::Uuid::new_v4().simple()));
  fs::create_dir(&path).unwrap();fs::create_dir(path.join("index")).unwrap();
  let store=Store{workspace:fs::File::open(&path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
  let current=format!("gen-{}",uuid::Uuid::new_v4().simple());let old=format!("gen-{}",uuid::Uuid::new_v4().simple());
  drop(store.create_gen(&current).unwrap());drop(store.create_gen(&old).unwrap());fs::write(path.join("index/current"),current.as_bytes()).unwrap();
  let mut child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","workspace_worker::fd_index_store::staging_tests::interrupted_retire_after_rename_recovers_without_removing_current","--nocapture"]).env("ATLAS_TEST_RETIRE_CHILD",&path).env("ATLAS_TEST_RETIRE_RENAME_BARRIER",&path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
  let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);while !path.join("ready").exists()&&std::time::Instant::now()<deadline{std::thread::sleep(std::time::Duration::from_millis(10));}
  if !path.join("ready").exists(){let _=child.kill();let _=child.wait();panic!("retire child did not reach rename barrier")}
  assert!(store.staging_guard().is_err());child.kill().unwrap();child.wait().unwrap();
  let guard=store.staging_guard().unwrap();assert!(path.join("index").join(&current).exists());assert!(!path.join("index").join(&old).exists());
  assert!(!crate::direct_export::entries(&store.root).unwrap().iter().any(|v|v.to_str().is_some_and(valid_retired)));
  drop(guard);drop(store);fs::remove_dir_all(path).unwrap();
 }
 #[test]
 fn many_generations_are_retired_in_bounded_batches() {
  let path=std::env::temp_dir().join(format!("atlas-retired-batches-{}",uuid::Uuid::new_v4().simple()));
  fs::create_dir(&path).unwrap();fs::create_dir(path.join("index")).unwrap();
  let store=Store{workspace:fs::File::open(&path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
  let current=format!("gen-{}",uuid::Uuid::new_v4().simple());let base=format!("gen-{}",uuid::Uuid::new_v4().simple());
  drop(store.create_gen(&current).unwrap());drop(store.create_gen(&base).unwrap());fs::write(path.join("index/current"),current.as_bytes()).unwrap();
  for _ in 0..25 {drop(store.create_gen(&format!("gen-{}",uuid::Uuid::new_v4().simple())).unwrap());}
  let _lease=store.staging_guard().unwrap();let mut calls=0;
  store.cleanup_batched(&current,Some(&base),||{calls+=1;Ok(())}).unwrap();
  assert_eq!(calls,5,"25 generations require four bounded batches and final check");
  let names=crate::direct_export::entries(&store.root).unwrap();let gens:Vec<_>=names.iter().filter(|v|v.to_str().is_some_and(valid_worker_generation)).collect();assert_eq!(gens.len(),2);
  assert!(path.join("index").join(&current).exists());assert!(path.join("index").join(&base).exists());
  drop(_lease);drop(store);fs::remove_dir_all(path).unwrap();
 }
 #[test]
 fn promotion_pins_stage_and_never_overwrites_generation() {
  let path=std::env::temp_dir().join(format!("atlas-store-stage-{}",uuid::Uuid::new_v4().simple()));
  fs::create_dir(&path).unwrap();fs::create_dir(path.join("index")).unwrap();
  let store=Store{workspace:fs::File::open(&path).unwrap(),root:fs::File::open(path.join("index")).unwrap(),name:"index".into()};
  let stage=format!(".stage-{}",uuid::Uuid::new_v4().simple());
  let target=format!("gen-{}",uuid::Uuid::new_v4().simple());
  let pinned=store.create_stage(&stage).unwrap();
  let other=store.create_gen(&target).unwrap();
  fs::write(path.join("index/current"),target.as_bytes()).unwrap();
  store.cleanup(&target,None).unwrap();
  assert!(path.join("index").join(&stage).is_dir(),"cleanup must preserve in-flight stage");
  assert!(store.promote_stage(&stage,&target,&pinned).is_err());
  assert!(store.promote_stage(&stage,&format!("gen-{}",uuid::Uuid::new_v4().simple()),&other).is_err());
  store.remove_gen(&target).unwrap();
  store.promote_stage(&stage,&target,&pinned).unwrap();
  store.verify_gen(&target,&pinned).unwrap();
  drop(store);fs::remove_dir_all(path).unwrap();
 }
}
