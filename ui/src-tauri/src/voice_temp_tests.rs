use super::*;
use std::{os::unix::{fs::{symlink,PermissionsExt},process::CommandExt},process::Command};
struct TestDir(PathBuf);
impl Drop for TestDir{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn temp(name:&str)->TestDir{let p=std::env::temp_dir().join(format!("atlas-voice-temp-test-{}-{name}",std::process::id()));fs::create_dir(&p).unwrap();TestDir(p)}
fn dead_pid()->u32{let mut child=Command::new("/usr/bin/true").spawn().unwrap();let pid=child.id();child.wait().unwrap();assert!(dead(pid));pid}
fn orphan(record:Recording){assert_eq!(unsafe{libc::flock(record._lock.as_ref().unwrap().as_raw_fd(),libc::LOCK_UN)},0);std::mem::forget(record)}

#[test]
fn normal_recording_is_removed_on_drop(){let t=temp("drop");let (record,file)=create(&t.0,std::process::id(),"abc").unwrap();drop(file);let path=record.path.clone();assert!(path.exists());drop(record);assert!(!path.exists());}

#[test]
fn abandoned_recording_is_removed_but_active_is_kept(){
 let t=temp("stale");let (old,f)=create(&t.0,dead_pid(),"old").unwrap();drop(f);let old_path=old.path.clone();orphan(old);
 let (active,f)=create(&t.0,std::process::id(),"active").unwrap();drop(f);let active_path=active.path.clone();
 cleanup(&t.0);assert!(!old_path.exists());assert!(active_path.exists());drop(active);
}

#[test]
fn unrelated_content_symlinks_and_unmarked_roots_are_preserved(){
 let t=temp("foreign");let outside=t.0.join("important");fs::write(&outside,b"keep").unwrap();
 let (record,f)=create(&t.0,dead_pid(),"foreign").unwrap();drop(f);let dir=record.dir.clone();fs::write(dir.join("unrelated"),b"keep").unwrap();orphan(record);
 cleanup(&t.0);assert!(dir.join("audio.wav").exists());assert_eq!(fs::read(dir.join("unrelated")).unwrap(),b"keep");
 let (record,f)=create(&t.0,dead_pid(),"link").unwrap();drop(f);let audio=record.path.clone();fs::remove_file(&audio).unwrap();symlink(&outside,&audio).unwrap();orphan(record);
 cleanup(&t.0);assert!(audio.is_symlink());assert_eq!(fs::read(&outside).unwrap(),b"keep");
 let unmarked=temp("unmarked");let fake=unmarked.0.join(ROOT);private_dir(&fake).unwrap();fs::set_permissions(&fake,fs::Permissions::from_mode(0o700)).unwrap();fs::write(fake.join("valuable"),b"keep").unwrap();cleanup(&unmarked.0);assert!(fake.join("valuable").exists());
}

#[test]
fn abandoned_parent_with_live_recognizer_keeps_audio(){
 let t=temp("locked");let (record,f)=create(&t.0,dead_pid(),"locked").unwrap();drop(f);let path=record.path.clone();
 cleanup(&t.0);assert!(path.exists(),"live inherited lock must protect the audio");
 orphan(record);cleanup(&t.0);assert!(!path.exists());
}

#[test]
fn recognizer_inherits_lock_after_parent_closes_its_copy(){
 let t=temp("inherited");let (record,f)=create(&t.0,dead_pid(),"inherited").unwrap();drop(f);
 let record=std::mem::ManuallyDrop::new(record);let path=record.path.clone();
 let fd=record.lock_fd();let mut command=Command::new("/bin/sleep");command.arg("3");
 unsafe{command.pre_exec(move||{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(std::io::Error::last_os_error())}Ok(())});}
 let mut child=command.spawn().unwrap();
 let lock=unsafe{std::ptr::read(&record._lock)};drop(lock);
 cleanup(&t.0);assert!(path.exists(),"child must retain the lock across exec");
 child.kill().unwrap();child.wait().unwrap();cleanup(&t.0);assert!(!path.exists());
}

#[test]
fn unrelated_child_does_not_inherit_lock(){
 let t=temp("unrelated-child");let (record,f)=create(&t.0,dead_pid(),"other").unwrap();drop(f);
 let record=std::mem::ManuallyDrop::new(record);let path=record.path.clone();let mut child=Command::new("/bin/sleep").arg("3").spawn().unwrap();
 let lock=unsafe{std::ptr::read(&record._lock)};drop(lock);cleanup(&t.0);assert!(!path.exists());child.kill().unwrap();child.wait().unwrap();
}

#[test]
fn interrupted_empty_root_initialization_does_not_block_next_recording(){
 let t=temp("root-create-crash");
 private_dir(&t.0.join(ROOT)).unwrap();
 let result=create(&t.0,std::process::id(),"recovered");
 assert!(result.is_ok(),"interrupted initialization of an empty root permanently blocks recording");
 assert!(fs::read_dir(t.0.join(ROOT)).unwrap().next().is_none(),"unmarked directory must remain untouched");
 let (record,file)=result.unwrap();drop(file);let chosen=record.dir.parent().unwrap().to_path_buf();
 let (next,next_file)=create(&t.0,std::process::id(),"next").unwrap();drop(next_file);assert_eq!(next.dir.parent().unwrap(),chosen);drop(next);drop(record);
}

#[test]
fn drop_keeps_audio_while_recognizer_inherits_lock(){
 let t=temp("drop-live-child");let (record,f)=create(&t.0,dead_pid(),"drop-child").unwrap();drop(f);
 let path=record.path.clone();let fd=record.lock_fd();let mut command=Command::new("/bin/sleep");command.arg("3");
 unsafe{command.pre_exec(move||{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(std::io::Error::last_os_error())}Ok(())});}
 let mut child=command.spawn().unwrap();drop(record);let survived=path.exists();
 child.kill().unwrap();child.wait().unwrap();
 assert!(survived,"audio was removed while the recognizer still held the lock");
 cleanup(&t.0);assert!(!path.exists(),"audio must be removable once the recognizer has stopped");
}

#[test]
fn fallback_is_reused_and_unmarked_root_is_preserved(){
 let t=temp("fallback-reuse");let foreign=t.0.join(ROOT);private_dir(&foreign).unwrap();
 fs::write(foreign.join("keep"),b"personal data").unwrap();
 let (first,f)=create(&t.0,std::process::id(),"first").unwrap();drop(f);
 let (second,f)=create(&t.0,std::process::id(),"second").unwrap();drop(f);
 assert_eq!(first.path.parent().unwrap().parent(),second.path.parent().unwrap().parent());
 assert_eq!(fs::read(foreign.join("keep")).unwrap(),b"personal data");assert!(!foreign.join("owner").exists());
 drop(first);drop(second);
}
