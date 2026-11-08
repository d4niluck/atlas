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
 while !ACTIVE.lock().unwrap().as_ref().is_some_and(|(key,_)|*key==id){assert!(Instant::now()<limit,"recognizer did not start");thread::sleep(Duration::from_millis(10));}
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
