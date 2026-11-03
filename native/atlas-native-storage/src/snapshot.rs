use std::{fs,io::{Read,Write},path::Path};
use sha2::{Digest,Sha256};
use super::{R,workspace_lock};

struct PrivateScratch(std::path::PathBuf,fs::File);
impl PrivateScratch {
 fn create()->R<Self>{use std::os::unix::fs::DirBuilderExt;let p=std::env::temp_dir().join(format!("atlas-snapshot-private-{}",uuid::Uuid::new_v4().simple()));fs::DirBuilder::new().mode(0o700).create(&p)?;let p=p.canonicalize()?;let dir=fs::File::open(&p)?;Ok(Self(p,dir))}
}
impl Drop for PrivateScratch{fn drop(&mut self){
 use std::os::fd::AsRawFd;
 // The directory handle stays bound to our scratch folder even if its name moves.
 for name in [c"snapshot.sqlite3",c"snapshot.sqlite3-journal",c"snapshot.sqlite3-wal",c"snapshot.sqlite3-shm"] {unsafe{libc::unlinkat(self.1.as_raw_fd(),name.as_ptr(),0);}}
 // Empty scratch folders are left for the operating system to clear.
}}

fn copy_open_file(mut input:fs::File,dst:&Path)->R<()> {
 let before=input.metadata()?;if !before.is_file(){return Err("unsupported file type".into())}
 let mut output=fs::OpenOptions::new().read(true).write(true).create_new(true).open(dst)?;
 let mut hash=Sha256::new();let mut buf=[0u8;65536];loop{let n=input.read(&mut buf)?;if n==0{break}output.write_all(&buf[..n])?;hash.update(&buf[..n]);}output.sync_all()?;
 use std::io::Seek;output.rewind()?;let mut check=output;let mut actual=Sha256::new();loop{let n=check.read(&mut buf)?;if n==0{break}actual.update(&buf[..n]);}if hash.finalize()!=actual.finalize(){return Err("copy verification failed".into())}use std::os::unix::fs::MetadataExt;let after=input.metadata()?;let identity=|m:&fs::Metadata|(m.dev(),m.ino(),m.len(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec());if identity(&before)!=identity(&after){return Err("source changed while copying".into())}Ok(())
}
#[cfg(feature="test-hook")]
fn snapshot_barrier(logical:&Path){if let (Ok(dir),Ok(target))=(std::env::var("ATLAS_TEST_SNAPSHOT_BARRIER"),std::env::var("ATLAS_TEST_SNAPSHOT_TARGET")){if logical==Path::new(&target){let dir=Path::new(&dir);let _=fs::write(dir.join("ready"),b"ready");let start=std::time::Instant::now();while !dir.join("resume").exists()&&start.elapsed()<std::time::Duration::from_secs(10){std::thread::sleep(std::time::Duration::from_millis(10));}}}}
#[cfg(not(feature="test-hook"))]
fn snapshot_barrier(_: &Path){}
fn copy_dir_fd(dir:&fs::File,dst:&Path,logical:&Path)->R<()> {
 use std::{ffi::CStr,os::{fd::{AsRawFd,FromRawFd},unix::ffi::OsStrExt}};
 struct Dir(*mut libc::DIR);impl Drop for Dir {fn drop(&mut self){unsafe{libc::closedir(self.0);}}}
 fs::create_dir(dst)?;let duplicated=unsafe{libc::dup(dir.as_raw_fd())};if duplicated<0{return Err(std::io::Error::last_os_error().into())}let stream=unsafe{libc::fdopendir(duplicated)};if stream.is_null(){unsafe{libc::close(duplicated)};return Err(std::io::Error::last_os_error().into())}let stream=Dir(stream);
 loop {#[cfg(target_os="macos")] unsafe{*libc::__error()=0};#[cfg(target_os="linux")] unsafe{*libc::__errno_location()=0};let item=unsafe{libc::readdir(stream.0)};if item.is_null(){let error=std::io::Error::last_os_error();if error.raw_os_error().unwrap_or(0)!=0{return Err(error.into())}break}let name=unsafe{CStr::from_ptr((*item).d_name.as_ptr())};let bytes=name.to_bytes();if bytes==b"."||bytes==b".."{continue}
  snapshot_barrier(&logical.join(std::ffi::OsStr::from_bytes(bytes)));
  let child=unsafe{libc::openat(dir.as_raw_fd(),name.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC)};if child<0{return Err(std::io::Error::last_os_error().into())}let child=unsafe{fs::File::from_raw_fd(child)};let metadata=child.metadata()?;let target=dst.join(std::ffi::OsStr::from_bytes(bytes));if metadata.is_dir(){copy_dir_fd(&child,&target,&logical.join(std::ffi::OsStr::from_bytes(bytes)))?}else if metadata.is_file(){copy_open_file(child,&target)?}else{return Err("unsupported file type".into())}
 }
 fs::File::open(dst)?.sync_all()?;Ok(())
}
fn copy_tree(src:&Path,dst:&Path)->R<()> {use std::os::unix::fs::OpenOptionsExt;let input=fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK).open(src)?;let metadata=input.metadata()?;if metadata.is_dir(){copy_dir_fd(&input,dst,Path::new(src.file_name().ok_or("invalid source")?))}else if metadata.is_file(){copy_open_file(input,dst)}else{Err("unsupported file type".into())}}
#[cfg(target_os="macos")]
pub fn publish_snapshot(src:&Path,dst:&Path)->std::io::Result<()> {use std::{ffi::CString,os::unix::ffi::OsStrExt};unsafe extern "C" {fn renamex_np(from:*const libc::c_char,to:*const libc::c_char,flags:libc::c_uint)->libc::c_int;}let a=CString::new(src.as_os_str().as_bytes())?;let b=CString::new(dst.as_os_str().as_bytes())?;if unsafe{renamex_np(a.as_ptr(),b.as_ptr(),4)}==0{Ok(())}else{Err(std::io::Error::last_os_error())}}
#[cfg(target_os="linux")]
pub fn publish_snapshot(src:&Path,dst:&Path)->std::io::Result<()> {use std::{ffi::CString,os::unix::ffi::OsStrExt};let a=CString::new(src.as_os_str().as_bytes())?;let b=CString::new(dst.as_os_str().as_bytes())?;if unsafe{libc::renameat2(libc::AT_FDCWD,a.as_ptr(),libc::AT_FDCWD,b.as_ptr(),libc::RENAME_NOREPLACE)}==0{Ok(())}else{Err(std::io::Error::last_os_error())}}
/// Create a verified, independent snapshot without replacing any existing destination.
pub fn snapshot_workspace(source:&Path,destination:&Path)->R<()> {snapshot_workspace_named(source,destination,None)}
pub fn snapshot_workspace_named(source:&Path,destination:&Path,workspace_name:Option<&str>)->R<()> {
 if source.is_symlink()||!source.is_dir(){return Err("invalid source workspace".into())}
 let source_resolved=source.canonicalize()?;let source=if source.is_absolute(){source_resolved.as_path()}else{source};let requested_parent=destination.parent().ok_or("missing destination parent")?;let parent_resolved=requested_parent.canonicalize()?;let parent=if requested_parent.is_absolute(){parent_resolved.as_path()}else{requested_parent};let name=destination.file_name().ok_or("missing destination name")?;let destination=parent.join(name);
 if parent_resolved.starts_with(&source_resolved)||destination.exists(){return Err("invalid or existing destination".into())}
 let _guard=workspace_lock(&source)?;if super::has_note_drafts(&source)?{return Err("Сначала завершите редактирование заметок и восстановление черновиков".into())}super::validate_database_files(source)?;let db_path=source.join("atlas.sqlite3");if db_path.is_symlink()||!db_path.is_file(){return Err("invalid workspace database".into())}
 let stage=parent.join(format!(".atlas-snapshot-{}",uuid::Uuid::new_v4().simple()));fs::create_dir(&stage)?;
 let result=(||->R<()>{
  let db=rusqlite::Connection::open_with_flags(&db_path,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW)?;let check:String=db.query_row("PRAGMA quick_check",[],|r|r.get(0))?;if check!="ok"{return Err("invalid workspace database".into())}
  let scratch=PrivateScratch::create()?;let scratch_db=scratch.0.join("snapshot.sqlite3");
  db.execute("VACUUM INTO ?",[scratch_db.to_str().ok_or("invalid path")?])?;drop(db);
  let checkdb=rusqlite::Connection::open_with_flags(&scratch_db,rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY|rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW)?;let check:String=checkdb.query_row("PRAGMA quick_check",[],|r|r.get(0))?;if check!="ok"{return Err("snapshot database verification failed".into())}drop(checkdb);copy_open_file(fs::File::open(&scratch_db)?,&stage.join("atlas.sqlite3"))?;
  for entry in fs::read_dir(&source)? {let entry=entry?;let name=entry.file_name();let s=name.to_string_lossy();if (s.starts_with(".atlas-upload-")||s.starts_with(".atlas-worker-upload-"))||matches!(s.as_ref(),"atlas.sqlite3"|"atlas.sqlite3-wal"|"atlas.sqlite3-shm"|"atlas.sqlite3-journal"|".atlas-operation.lock"|".atlas-session.lock"|".atlas-workspace-id"|".atlas-semantic-native"|".atlas-worker-index"){continue}copy_tree(&entry.path(),&stage.join(name))?;}
  {let mut f=fs::OpenOptions::new().write(true).create_new(true).open(stage.join(".atlas-workspace-id"))?;f.write_all(uuid::Uuid::new_v4().simple().to_string().as_bytes())?;f.sync_all()?;}
  for index_name in [".atlas-semantic-native",".atlas-worker-index"]{let index=source.join(index_name);#[cfg(feature="test-hook")] if std::env::var_os("ATLAS_TEST_SNAPSHOT_INDEX_STAT_EIO").is_some(){return Err(std::io::Error::from_raw_os_error(libc::EIO).into())}let has_index=match fs::symlink_metadata(&index){Ok(_)=>true,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>false,Err(e)=>return Err(e.into())};if has_index{use std::{os::{fd::{AsRawFd,FromRawFd},unix::fs::OpenOptionsExt},io::Seek};let index=fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW|libc::O_DIRECTORY).open(&index)?;let current_name=c"current";let current=unsafe{libc::openat(index.as_raw_fd(),current_name.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC)};if current<0 {let e=std::io::Error::last_os_error();if e.kind()!=std::io::ErrorKind::NotFound{return Err(e.into())}}else{let mut current=unsafe{fs::File::from_raw_fd(current)};if !current.metadata()?.is_file(){return Err("invalid search index".into())}let mut generation=String::new();current.read_to_string(&mut generation)?;if !generation.starts_with("gen-")||!generation.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'){return Err("invalid search index".into())}let name=std::ffi::CString::new(generation.as_bytes())?;let fd=unsafe{libc::openat(index.as_raw_fd(),name.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_DIRECTORY|libc::O_CLOEXEC)};if fd<0{return Err(std::io::Error::last_os_error().into())}let generation_dir=unsafe{fs::File::from_raw_fd(fd)};let out=stage.join(index_name);fs::create_dir(&out)?;current.rewind()?;copy_open_file(current,&out.join("current"))?;copy_dir_fd(&generation_dir,&out.join(&generation),Path::new(index_name).join(&generation).as_path())?;fs::File::open(out)?.sync_all()?;}}}
  if let Some(name)=workspace_name {let marker=stage.join(".workspace.json");fs::write(&marker,serde_json::to_vec(&serde_json::json!({"name":name}))?)?;fs::File::open(marker)?.sync_all()?;}
  fs::File::open(&stage)?.sync_all()?;publish_snapshot(&stage,&destination)?;if let Err(e)=fs::File::open(&parent).and_then(|f|f.sync_all()){return Err(format!("snapshot published but directory sync failed: {e}").into())}Ok(())
 })();result
}
