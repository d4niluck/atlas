use std::{fs::{self,DirBuilder,File,OpenOptions},io::Write,os::{fd::AsRawFd,unix::fs::{DirBuilderExt,MetadataExt,OpenOptionsExt}},path::{Path,PathBuf}};

const ROOT:&str="atlas-voice-recordings-v1";
const MARKER:&str="Atlas voice recording v1\n";
fn owned_private(path:&Path,dir:bool)->bool{
 fs::symlink_metadata(path).is_ok_and(|m|m.uid()==unsafe{libc::geteuid()} && (m.mode()&0o077)==0 && if dir{m.file_type().is_dir()}else{m.file_type().is_file()})
}
fn private_dir(path:&Path)->std::io::Result<()>{DirBuilder::new().mode(0o700).create(path)}
fn write_marker(path:&Path)->std::io::Result<()>{let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;f.write_all(MARKER.as_bytes())}
fn has_marker(path:&Path)->bool{owned_private(path,false)&&fs::symlink_metadata(path).is_ok_and(|m|m.len()==MARKER.len() as u64)&&fs::read(path).is_ok_and(|v|v==MARKER.as_bytes())}

fn root(temp:&Path,create:bool)->std::io::Result<Option<PathBuf>>{
 let root=temp.join(ROOT);
 if create {match private_dir(&root){Ok(())=>write_marker(&root.join("owner"))?,Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{},Err(e)=>return Err(e)}}
 if !root.exists(){return Ok(None)}
 if !owned_private(&root,true)||!has_marker(&root.join("owner")){return Err(std::io::Error::other("Небезопасная временная папка голосового ввода"))}
 Ok(Some(root))
}

pub struct Recording{dir:PathBuf,pub path:PathBuf,_lock:File}
impl Recording{pub fn lock_fd(&self)->i32{self._lock.as_raw_fd()}}
impl Drop for Recording{fn drop(&mut self){remove_recording(&self.dir)}}
pub fn create(temp:&Path,pid:u32,id:&str)->std::io::Result<(Recording,File)>{
 let root=root(temp,true)?.ok_or_else(||std::io::Error::other("Нет временной папки"))?;
 let dir=root.join(format!("{pid}-{id}"));private_dir(&dir)?;
 if let Err(e)=write_marker(&dir.join("owner")){let _=fs::remove_dir(&dir);return Err(e)}
 let lock=OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("lock"))?;
 if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err(std::io::Error::last_os_error())}
 let record=Recording{path:dir.join("audio.wav"),dir,_lock:lock};
 let file=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&record.path)?;
 Ok((record,file))
}
fn remove_recording(dir:&Path){
 if !owned_private(dir,true)||!has_marker(&dir.join("owner")){return}
 let Ok(entries)=fs::read_dir(dir) else{return};
 for entry in entries{let Ok(entry)=entry else{return};let name=entry.file_name();if name!="owner"&&name!="audio.wav"&&name!="lock"{return}if !owned_private(&entry.path(),false){return}}
 let audio=dir.join("audio.wav");if audio.exists(){let _=fs::remove_file(audio);}
 let _=fs::remove_file(dir.join("lock"));let _=fs::remove_file(dir.join("owner"));let _=fs::remove_dir(dir);
}
fn dead(pid:u32)->bool{if pid==0||pid>i32::MAX as u32{return false}let result=unsafe{libc::kill(pid as i32,0)};result==-1&&std::io::Error::last_os_error().raw_os_error()==Some(libc::ESRCH)}
pub fn cleanup(temp:&Path){
 let Ok(Some(root))=root(temp,false)else{return};let Ok(entries)=fs::read_dir(root)else{return};
 for entry in entries.flatten(){let name=entry.file_name();let Some(name)=name.to_str()else{continue};let Some((pid,id))=name.split_once('-')else{continue};if id.is_empty()||id.len()>64||!id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'){continue}let Ok(pid)=pid.parse::<u32>()else{continue};if !dead(pid){continue}let path=entry.path();let lock=path.join("lock");if !owned_private(&lock,false){continue}let Ok(file)=OpenOptions::new().write(true).custom_flags(libc::O_NOFOLLOW).open(lock)else{continue};if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0{remove_recording(&path)}}
}
#[cfg(test)]
#[path="voice_temp_tests.rs"]
mod tests;
