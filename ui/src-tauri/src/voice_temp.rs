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
 if !create{return Ok(if owned_private(&root,true)&&has_marker(&root.join("owner")){Some(root)}else{None})}
 match private_dir(&root){Ok(())=>{write_marker(&root.join("owner"))?;return Ok(Some(root))},Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{},Err(e)=>return Err(e)}
 if owned_private(&root,true)&&has_marker(&root.join("owner")){return Ok(Some(root))}
 // An unmarked directory is not ours, even if empty. Create a separate one.
 for n in 0..1000{let candidate=temp.join(format!("{ROOT}-{}-{n}",std::process::id()));match private_dir(&candidate){Ok(())=>{write_marker(&candidate.join("owner"))?;return Ok(Some(candidate))},Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{if owned_private(&candidate,true)&&has_marker(&candidate.join("owner")){return Ok(Some(candidate))}},Err(e)=>return Err(e)}}
 Err(std::io::Error::other("Не удалось создать временную папку голосового ввода"))
}

pub struct Recording{dir:PathBuf,pub path:PathBuf,_lock:Option<File>}
impl Recording{pub fn lock_fd(&self)->i32{self._lock.as_ref().expect("active lock").as_raw_fd()}}
impl Drop for Recording{fn drop(&mut self){drop(self._lock.take());remove_when_unlocked(&self.dir)}}
pub fn create(temp:&Path,pid:u32,id:&str)->std::io::Result<(Recording,File)>{
 let root=root(temp,true)?.ok_or_else(||std::io::Error::other("Нет временной папки"))?;
 let dir=root.join(format!("{pid}-{id}"));private_dir(&dir)?;
 if let Err(e)=write_marker(&dir.join("owner")){let _=fs::remove_dir(&dir);return Err(e)}
 let lock=OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("lock"))?;
 if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err(std::io::Error::last_os_error())}
 let record=Recording{path:dir.join("audio.wav"),dir,_lock:Some(lock)};
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
fn remove_when_unlocked(dir:&Path){
 if !owned_private(dir,true)||!has_marker(&dir.join("owner")){return}
 let lock=dir.join("lock");if !owned_private(&lock,false){return}
 let Ok(file)=OpenOptions::new().write(true).custom_flags(libc::O_NOFOLLOW).open(lock)else{return};
 if unsafe{libc::flock(file.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0{remove_recording(dir)}
}
fn dead(pid:u32)->bool{if pid==0||pid>i32::MAX as u32{return false}let result=unsafe{libc::kill(pid as i32,0)};result==-1&&std::io::Error::last_os_error().raw_os_error()==Some(libc::ESRCH)}
pub fn cleanup(temp:&Path){
 let Ok(roots)=fs::read_dir(temp)else{return};
 for root in roots.flatten(){let name=root.file_name();let Some(name)=name.to_str()else{continue};if name!=ROOT&&!name.starts_with(&format!("{ROOT}-")){continue}let path=root.path();if !owned_private(&path,true)||!has_marker(&path.join("owner")){continue}let Ok(entries)=fs::read_dir(path)else{continue};
  for entry in entries.flatten(){let name=entry.file_name();let Some(name)=name.to_str()else{continue};let Some((pid,id))=name.split_once('-')else{continue};if id.is_empty()||id.len()>64||!id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'){continue}let Ok(pid)=pid.parse::<u32>()else{continue};if dead(pid){remove_when_unlocked(&entry.path())}}
 }
}
#[cfg(test)]
#[path="voice_temp_tests.rs"]
mod tests;
