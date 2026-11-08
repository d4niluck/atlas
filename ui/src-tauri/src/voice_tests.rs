use super::*;
use std::{os::unix::fs::PermissionsExt, thread, time::{Duration,Instant}};

struct TestDir(PathBuf);
impl std::ops::Deref for TestDir{type Target=Path;fn deref(&self)->&Path{&self.0}}
impl Drop for TestDir{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn temporary(name:&str)->TestDir{
 let p=std::env::temp_dir().join(format!("atlas-voice-tests-{}-{name}",std::process::id()));
 std::fs::create_dir(&p).expect("test directory must not exist");TestDir(p)
}

#[test]
fn model_sizes_and_unknown_quality(){
 for quality in ["base","small","medium"]{let(name,sha,size)=model(quality).unwrap();assert!(name.ends_with(".bin"));assert_eq!(sha.len(),64);assert!(size>1_000_000)}
 assert!(model("base.en").is_err());assert!(model("unknown").is_err());
}

#[test]
fn existing_incomplete_model_is_not_accepted(){
 let dir=temporary("corrupt");let(name,sha,size)=model("base").unwrap();
 std::fs::write(dir.join(name),b"partial").unwrap();
 assert!(download(&dir.0,name,sha,size).is_err(),"an existing but incomplete model must not be accepted as installed");

}

#[test]
fn cancelled_before_launch_does_not_start_process(){
 let dir=temporary("precancel");let exe=dir.join("mock");let marker=dir.join("started");
 std::fs::write(&exe,format!("#!/bin/sh\nprintf started > '{}'\n",marker.display())).unwrap();
 std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("precancel-{}",std::process::id());voice_cancel(id.clone()).unwrap();
 let result=transcribe(&exe,&dir.join("model"),&id,b"RIFF fake");
 assert!(result.is_err());assert!(!marker.exists(),"cancelled request must not start recognizer");
 assert!(!std::env::temp_dir().join(format!("atlas-voice-{}-{id}.wav",std::process::id())).exists());

}

#[test]
fn cancellation_terminates_child_and_removes_temporary_audio(){
 let dir=temporary("kill");let exe=dir.join("mock");
 std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 3\n").unwrap();
 std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("kill-{}",std::process::id());let thread_id=id.clone();let model=dir.join("model");
 let worker=thread::spawn(move||transcribe(&exe,&model,&thread_id,b"RIFF fake"));
 let limit=Instant::now()+Duration::from_secs(2);
 while !ACTIVE.lock().unwrap().as_ref().is_some_and(|entry|entry.0==id){assert!(Instant::now()<limit,"recognizer did not start");thread::sleep(Duration::from_millis(10));}
 let start=Instant::now();voice_cancel(id.clone()).unwrap();assert!(worker.join().unwrap().is_err());
 assert!(start.elapsed()<Duration::from_secs(2),"cancel must not wait for the full decode");
 assert!(!std::env::temp_dir().join(format!("atlas-voice-{}-{id}.wav",std::process::id())).exists());
 assert!(ACTIVE.lock().unwrap().is_none());
}

#[test]
fn same_sized_corrupt_model_is_not_accepted(){
 let dir=temporary("wrong-hash");let(name,sha,size)=model("base").unwrap();
 std::fs::File::create(dir.join(name)).unwrap().set_len(size).unwrap();
 assert!(download(&dir.0,name,sha,size).is_err(),"file length alone does not prove model integrity");
}

#[test]
fn status_distinguishes_missing_valid_and_corrupt_model(){
 let dir=temporary("status");let path=dir.join("model");
 const SHA:&str="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
 assert_eq!(model_status(&path,SHA,3).unwrap(),"missing");
 std::fs::write(&path,b"bad").unwrap();
 assert_eq!(model_status(&path,SHA,3).unwrap(),"corrupt");
 std::fs::write(&path,b"abc").unwrap();
 assert_eq!(model_status(&path,SHA,3).unwrap(),"installed");
 std::fs::write(&path,b"ab").unwrap();
 assert_eq!(model_status(&path,SHA,3).unwrap(),"corrupt");
}

#[test]
fn shutdown_cancels_active_recognizer_and_cleans_recording(){
 let _reset=ShutdownReset;SHUTTING_DOWN.store(false,std::sync::atomic::Ordering::SeqCst);
 let dir=temporary("shutdown");let exe=dir.join("mock");
 std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 3\n").unwrap();
 std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("shutdown-{}",std::process::id());let thread_id=id.clone();let model=dir.join("model");
 let worker=thread::spawn(move||transcribe(&exe,&model,&thread_id,b"RIFF fake"));
 let limit=Instant::now()+Duration::from_secs(2);
 while !ACTIVE.lock().unwrap().as_ref().is_some_and(|entry|entry.0==id){assert!(Instant::now()<limit,"recognizer did not start");thread::sleep(Duration::from_millis(10));}
 let begin=Instant::now();shutdown();assert!(worker.join().unwrap().is_err());
 assert!(begin.elapsed()<Duration::from_secs(2));assert!(ACTIVE.lock().unwrap().is_none());
 assert!(!std::env::temp_dir().join(format!("atlas-voice-{}-{id}.wav",std::process::id())).exists());
}

#[test]
fn shutdown_terminates_download_and_removes_partial_file(){
 let _reset=ShutdownReset;SHUTTING_DOWN.store(false,std::sync::atomic::Ordering::SeqCst);
 let dir=temporary("shutdown-download");let path=dir.join("partial");std::fs::write(&path,b"partial").unwrap();
 let child=Command::new("/bin/sleep").arg("3").spawn().unwrap();let pid=child.id() as libc::pid_t;
 *DOWNLOAD_CHILD.lock().unwrap()=Some((child,path.clone()));
 let begin=Instant::now();shutdown();
 assert!(begin.elapsed()<Duration::from_secs(2));assert!(DOWNLOAD_CHILD.lock().unwrap().is_none());
 assert!(!path.exists());
 assert_eq!(unsafe{libc::kill(pid,0)},-1,"download process must not remain after shutdown");
 assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH),"an access error does not prove the process is gone");
}

struct ShutdownReset;
impl Drop for ShutdownReset{fn drop(&mut self){SHUTTING_DOWN.store(false,std::sync::atomic::Ordering::SeqCst);}}

#[test]
fn shutdown_prevents_starting_new_recognizer(){
 let _reset=ShutdownReset;let dir=temporary("shutdown-before-start");let exe=dir.join("mock");let marker=dir.join("started");
 std::fs::write(&exe,format!("#!/bin/sh\nprintf started > '{}'\n",marker.display())).unwrap();
 std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 shutdown();let id=format!("shutdown-before-{}",std::process::id());
 assert!(transcribe(&exe,&dir.join("model"),&id,b"RIFF fake").is_err());
 assert!(!marker.exists());
 assert!(!std::env::temp_dir().join(format!("atlas-voice-{}-{id}.wav",std::process::id())).exists());
}
