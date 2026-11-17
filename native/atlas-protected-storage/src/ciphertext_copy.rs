//! Exclusive ciphertext copy for publishing a prepared encrypted index.
//! The caller validates source revision under its own workspace guard before
//! exposing the returned file through its durable primary marker.
use std::{fs::{File,OpenOptions},io::{self,Read,Seek,SeekFrom,Write},os::{fd::{AsRawFd,FromRawFd},unix::{ffi::OsStrExt,fs::{MetadataExt,OpenOptionsExt}}},path::Path};
#[cfg(any(test,feature="test-hook"))]
use std::fs;
use sha2::{Digest,Sha256};
use std::ffi::CString;

fn pin_parent(destination:&Path)->Result<(File,CString),String>{
 let parent=destination.parent().ok_or("Нет папки индекса")?;
 let name=destination.file_name().ok_or("Некорректное имя файла")?;
 let name=CString::new(name.as_bytes()).map_err(|_|"Некорректное имя файла")?;
 let dir=OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC).open(parent).map_err(|e|e.to_string())?;
 Ok((dir,name))
}
fn open_at(dir:&File,name:&CString,flags:i32)->Result<File,String>{
 let fd=unsafe{libc::openat(dir.as_raw_fd(),name.as_ptr(),flags|libc::O_NOFOLLOW|libc::O_CLOEXEC,0o600)};
 if fd<0{return Err(io::Error::last_os_error().to_string())}Ok(unsafe{File::from_raw_fd(fd)})
}

#[cfg(feature="test-hook")]
fn copy_barrier(point:&str){if let Some(dir)=std::env::var_os("ATLAS_TEST_COPY_BARRIER"){let dir=Path::new(&dir);let _=fs::write(dir.join(format!("{point}-ready")),b"ready");while !dir.join(format!("{point}-go")).exists(){std::thread::sleep(std::time::Duration::from_millis(10));}}}

/// Copy encrypted bytes directly to an exclusively created destination. The
/// durable record is committed before and after creation; an incomplete file is
/// never marked ready, and an unrecognised file is never deleted automatically.
pub fn begin_recorded_copy(primary:&rusqlite::Connection, destination:&Path, generation:&str)->Result<(),String>{
 let name=destination.file_name().and_then(|s|s.to_str()).ok_or("Некорректное имя файла")?;
 if generation.len()!=32||!generation.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Некорректное поколение индекса".into())}
 primary.execute_batch("CREATE TABLE IF NOT EXISTS atlas_fts_migration (generation TEXT PRIMARY KEY, name TEXT NOT NULL, device INTEGER, inode INTEGER, digest BLOB, ready INTEGER NOT NULL CHECK(ready IN (0,1)))").map_err(|e|e.to_string())?;
 primary.execute("INSERT INTO atlas_fts_migration(generation,name,ready) VALUES(?1,?2,0)",rusqlite::params![generation,name]).map_err(|e|e.to_string())?;Ok(())
}

#[cfg(any(test,feature="test-hook"))]
pub fn prepare_recorded_copy(source:&File, primary:&rusqlite::Connection, destination:&Path, generation:&str, verify:impl FnMut()->Result<(),String>, marker_change:impl FnMut(&mut dyn FnMut()->Result<(),String>)->Result<(),String>)->Result<File,String> {prepare_recorded_copy_anchored(source,primary,destination,generation,verify,marker_change,|_|Ok(()))}
pub fn prepare_recorded_copy_anchored(source:&File, primary:&rusqlite::Connection, destination:&Path, generation:&str, mut verify:impl FnMut()->Result<(),String>, mut marker_change:impl FnMut(&mut dyn FnMut()->Result<(),String>)->Result<(),String>, verify_parent:impl Fn(&File)->Result<(),String>)->Result<File,String> {
 use rusqlite::params;
 let name=destination.file_name().and_then(|s|s.to_str()).ok_or("Некорректное имя файла")?;
 verify()?;
 let marker:(String,i64)=primary.query_row("SELECT name,ready FROM atlas_fts_migration WHERE generation=?1",[generation],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|"Нет записи подготовки индекса")?;
 if marker.0!=name||marker.1!=0{return Err("Запись подготовки индекса не соответствует файлу".into())}
 verify()?;
 let (dir,entry)=pin_parent(destination)?;verify_parent(&dir)?;verify()?;
 let mut output=open_at(&dir,&entry,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
 #[cfg(feature="test-hook")] copy_barrier("created");
 let meta=output.metadata().map_err(|e|e.to_string())?;
 if !meta.is_file(){return Err("Некорректный файл индекса".into())}
 verify()?;
 marker_change(&mut ||{if primary.execute("UPDATE atlas_fts_migration SET device=?1,inode=?2 WHERE generation=?3 AND ready=0 AND device IS NULL AND inode IS NULL",params![meta.dev() as i64,meta.ino() as i64,generation]).map_err(|e|e.to_string())?!=1{return Err("Запись подготовки индекса изменилась".into())}Ok(())})?;
 #[cfg(feature="test-hook")] copy_barrier("identified");
 let mut input=source.try_clone().map_err(|e|e.to_string())?;input.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;
 let mut hash=Sha256::new();let mut buf=[0u8;65536];
 loop {verify()?;let n=input.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break}hash.update(&buf[..n]);output.write_all(&buf[..n]).map_err(|e|e.to_string())?;}
 #[cfg(feature="test-hook")] copy_barrier("copied");
 output.sync_all().map_err(|e|e.to_string())?;
 let current=open_at(&dir,&entry,libc::O_RDONLY)?.metadata().map_err(|e|e.to_string())?;
 if !current.is_file()||(current.dev(),current.ino())!=(meta.dev(),meta.ino()){return Err("Файл индекса изменился".into())}
 dir.sync_all().map_err(|e|e.to_string())?;
 verify_parent(&dir)?;verify()?;let digest=hash.finalize();
 marker_change(&mut ||{if primary.execute("UPDATE atlas_fts_migration SET digest=?1,ready=1 WHERE generation=?2 AND device=?3 AND inode=?4 AND ready=0",params![digest.as_slice(),generation,meta.dev() as i64,meta.ino() as i64]).map_err(|e|e.to_string())?!=1{return Err("Запись подготовки индекса изменилась".into())}Ok(())})?;
 #[cfg(feature="test-hook")] copy_barrier("ready");
 Ok(output)
}

/// Fail closed on missing, partial, replaced or modified ciphertext after restart.
#[cfg(any(test,feature="test-hook"))]
pub fn open_recorded_copy(primary:&rusqlite::Connection,destination:&Path,generation:&str)->Result<File,String>{open_recorded_copy_with(primary,destination,generation,||Ok(()))}
#[cfg(any(test,feature="test-hook"))]
pub fn open_recorded_copy_with(primary:&rusqlite::Connection,destination:&Path,generation:&str,verify:impl FnMut()->Result<(),String>)->Result<File,String>{open_recorded_copy_anchored(primary,destination,generation,verify,|_|Ok(()))}
pub fn open_recorded_copy_anchored(primary:&rusqlite::Connection,destination:&Path,generation:&str,mut verify:impl FnMut()->Result<(),String>,verify_parent:impl Fn(&File)->Result<(),String>)->Result<File,String>{
 let (name,device,inode,digest,ready):(String,Option<i64>,Option<i64>,Option<Vec<u8>>,i64)=primary.query_row("SELECT name,device,inode,digest,ready FROM atlas_fts_migration WHERE generation=?1",[generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|_|"Запись подготовки индекса не найдена")?;
 if ready!=1||destination.file_name().and_then(|s|s.to_str())!=Some(name.as_str()){return Err("Подготовка индекса не завершена".into())}
 let (dir,entry)=pin_parent(destination)?;verify_parent(&dir)?;
 let mut file=open_at(&dir,&entry,libc::O_RDONLY)?;
 let meta=file.metadata().map_err(|e|e.to_string())?;
 if !meta.is_file()||Some(meta.dev() as i64)!=device||Some(meta.ino() as i64)!=inode{return Err("Файл индекса изменился".into())}
 let mut hash=Sha256::new();let mut buf=[0u8;65536];loop{verify()?;let n=file.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break}hash.update(&buf[..n]);}
 let actual=hash.finalize();if digest.as_deref()!=Some(actual.as_slice()){return Err("Файл индекса повреждён".into())}
 let current=open_at(&dir,&entry,libc::O_RDONLY)?.metadata().map_err(|e|e.to_string())?;
 if !current.is_file()||(current.dev(),current.ino())!=(meta.dev(),meta.ino()){return Err("Файл индекса изменился".into())}
 verify_parent(&dir)?;file.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;Ok(file)
}

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn recorded_copy_marks_ready_only_after_complete_copy() {
  let d=std::env::temp_dir().join(format!("atlas-recorded-copy-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();
  let source=d.join("source");fs::write(&source,b"encrypted payload").unwrap();let input=File::open(source).unwrap();
  let primary=rusqlite::Connection::open_in_memory().unwrap();let destination=d.join("index");
  let generation="0123456789abcdef0123456789abcdef";begin_recorded_copy(&primary,&destination,generation).unwrap();let _pin=prepare_recorded_copy(&input,&primary,&destination,generation,||Ok(()),|write|write()).unwrap();
  let ready:i64=primary.query_row("SELECT ready FROM atlas_fts_migration WHERE generation='0123456789abcdef0123456789abcdef'",[],|r|r.get(0)).unwrap();assert_eq!(ready,1);assert_eq!(fs::read(&destination).unwrap(),b"encrypted payload");assert!(open_recorded_copy(&primary,&destination,generation).is_ok());fs::write(&destination,b"corrupt").unwrap();assert!(open_recorded_copy(&primary,&destination,generation).is_err());drop(_pin);fs::remove_dir_all(d).unwrap();
 }
 #[test] fn recorded_copy_cancel_never_marks_ready() {
  let d=std::env::temp_dir().join(format!("atlas-recorded-cancel-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();
  let source=d.join("source");fs::write(&source,b"encrypted payload").unwrap();let input=File::open(source).unwrap();let primary=rusqlite::Connection::open_in_memory().unwrap();
  let generation="0123456789abcdef0123456789abcdef";begin_recorded_copy(&primary,&d.join("index"),generation).unwrap();let mut calls=0;assert!(prepare_recorded_copy(&input,&primary,&d.join("index"),generation,||{calls+=1;if calls==5{Err("cancelled".into())}else{Ok(())}},|write|write()).is_err());
  let ready:i64=primary.query_row("SELECT ready FROM atlas_fts_migration WHERE generation='0123456789abcdef0123456789abcdef'",[],|r|r.get(0)).unwrap();assert_eq!(ready,0);fs::remove_dir_all(d).unwrap();
 }
 #[test] fn hash_checks_cancellation_between_blocks(){
  let d=std::env::temp_dir().join(format!("atlas-copy-hash-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();let source=d.join("source");fs::write(&source,vec![42u8;200_000]).unwrap();let input=File::open(source).unwrap();let primary=rusqlite::Connection::open_in_memory().unwrap();let dest=d.join("index");let generation="0123456789abcdef0123456789abcdef";begin_recorded_copy(&primary,&dest,generation).unwrap();drop(prepare_recorded_copy(&input,&primary,&dest,generation,||Ok(()),|write|write()).unwrap());let mut calls=0;let result=open_recorded_copy_with(&primary,&dest,generation,||{calls+=1;if calls==3{Err("cancelled".into())}else{Ok(())}});assert!(result.is_err());assert_eq!(calls,3);fs::remove_dir_all(d).unwrap();
 }
 #[test] fn replaced_parent_before_pin_is_rejected(){
  let root=std::env::temp_dir().join(format!("atlas-copy-prepin-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let dir=root.join("parent");fs::create_dir(&dir).unwrap();let expected=fs::metadata(&dir).unwrap();let source=root.join("source");fs::write(&source,b"ciphertext").unwrap();let input=File::open(source).unwrap();let primary=rusqlite::Connection::open_in_memory().unwrap();let dest=dir.join("index");let generation="0123456789abcdef0123456789abcdef";begin_recorded_copy(&primary,&dest,generation).unwrap();let mut calls=0;
  let result=prepare_recorded_copy_anchored(&input,&primary,&dest,generation,||{calls+=1;if calls==2{fs::rename(&dir,root.join("original")).unwrap();fs::create_dir(&dir).unwrap();fs::write(dir.join("sentinel"),b"user data").unwrap();}Ok(())},|write|write(),|p|{let m=p.metadata().map_err(|e|e.to_string())?;if (m.dev(),m.ino())!=(expected.dev(),expected.ino()){Err("directory changed".into())}else{Ok(())}});assert!(result.is_err());assert_eq!(fs::read(dir.join("sentinel")).unwrap(),b"user data");assert!(!dest.exists());fs::remove_dir_all(root).unwrap();
 }
 #[test] fn parent_swap_does_not_write_into_replacement(){
  let root=std::env::temp_dir().join(format!("atlas-copy-parent-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let dir=root.join("parent");fs::create_dir(&dir).unwrap();let source=root.join("source");fs::write(&source,b"ciphertext").unwrap();let input=File::open(source).unwrap();let primary=rusqlite::Connection::open_in_memory().unwrap();let dest=dir.join("index");let generation="0123456789abcdef0123456789abcdef";begin_recorded_copy(&primary,&dest,generation).unwrap();let mut calls=0;
  let result=prepare_recorded_copy(&input,&primary,&dest,generation,||{calls+=1;if calls==5{fs::rename(&dir,root.join("original")).unwrap();fs::create_dir(&dir).unwrap();fs::write(dir.join("sentinel"),b"user data").unwrap();return Err("directory changed".into())}Ok(())},|write|write());assert!(result.is_err());assert_eq!(fs::read(dir.join("sentinel")).unwrap(),b"user data");assert!(!dest.exists());fs::remove_dir_all(root).unwrap();
 }
 #[test] fn replaced_destination_does_not_damage_foreign_file() {
  let d=std::env::temp_dir().join(format!("atlas-recorded-swap-{}",uuid::Uuid::new_v4()));fs::create_dir(&d).unwrap();
  let source=d.join("source");fs::write(&source,b"encrypted payload").unwrap();let input=File::open(source).unwrap();let primary=rusqlite::Connection::open_in_memory().unwrap();
  let destination=d.join("index");let held=d.join("original");let foreign=d.join("foreign");fs::write(&foreign,b"user data").unwrap();
  let generation="0123456789abcdef0123456789abcdef";begin_recorded_copy(&primary,&destination,generation).unwrap();let mut count=0;
  let result=prepare_recorded_copy(&input,&primary,&destination,generation,||{count+=1;if count==5{fs::rename(&destination,&held).unwrap();fs::hard_link(&foreign,&destination).unwrap();}Ok(())},|write|write());
  assert!(result.is_err());assert_eq!(fs::read(&foreign).unwrap(),b"user data");assert_eq!(fs::read(&destination).unwrap(),b"user data");assert!(open_recorded_copy(&primary,&destination,generation).is_err());fs::remove_dir_all(d).unwrap();
 }
}
