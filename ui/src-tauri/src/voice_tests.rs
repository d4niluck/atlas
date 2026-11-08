use super::*;
use std::{os::unix::fs::PermissionsExt, thread, time::{Duration,Instant}};

#[test]
fn silence_markers_do_not_replace_the_search_query(){
 assert_eq!(clean_voice_output("  [BLANK_AUDIO]\n"),"");
 assert_eq!(clean_voice_output("[BLANK_AUDIO] [BLANK_AUDIO]"),"");
 assert_eq!(clean_voice_output("  Найди заметку про RAG.  "),"Найди заметку про RAG.");
 assert_eq!(clean_voice_output("[BLANK_AUDIO] Найди заметку"),"Найди заметку");
}

#[test]
fn digital_silence_is_skipped_but_quiet_pcm_is_not(){
 assert!(pcm_is_digital_silence(&[0,0,0,0]));
 assert!(!pcm_is_digital_silence(&[0,0,1,0]));
 assert!(!pcm_is_digital_silence(&[0,0,255,255]));
}

#[test]
fn warm_recognizer_starts_before_audio_and_accepts_stdin(){
 let dir=temporary("warm");let exe=dir.join("mock");let marker=dir.join("started");let model=dir.join("model");
 std::fs::write(&exe,format!("#!/bin/sh\nprintf started > '{}'\n/bin/cat >/dev/null\nprintf recognized\n",marker.display())).unwrap();
 std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();let id=format!("warm-{}",std::process::id());
 prepare_process(&exe,&model,&id).unwrap();let end=Instant::now()+Duration::from_secs(2);while !marker.exists(){assert!(Instant::now()<end);thread::sleep(Duration::from_millis(5));}
 assert_eq!(finish_prepared(&model,&id,b"RIFF sample").unwrap().unwrap(),"recognized");
 assert!(PREWARM.lock().unwrap().is_none());
}

#[test]
fn cancelled_preparation_cannot_start_a_recognizer(){
 let dir=temporary("warm-precancel");let exe=dir.join("mock");let marker=dir.join("started");
 std::fs::write(&exe,format!("#!/bin/sh\nprintf started > '{}'\n",marker.display())).unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-precancel-{}",std::process::id());voice_cancel(id.clone()).unwrap();assert!(prepare_process(&exe,&dir.join("model"),&id).is_err());assert!(!marker.exists());
}

#[test]
fn cancelling_warmed_recognizer_reaps_it_before_any_audio(){
 let dir=temporary("warm-cancel");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-cancel-{}",std::process::id());prepare_process(&exe,&dir.join("model"),&id).unwrap();let pid=PREWARM.lock().unwrap().as_ref().unwrap().child.id() as libc::pid_t;
 voice_cancel(id).unwrap();assert!(PREWARM.lock().unwrap().is_none());assert_eq!(unsafe{libc::kill(pid,0)},-1);assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));
}

#[test]
fn cancellation_unblocks_audio_write_when_model_is_still_loading(){
 let dir=temporary("warm-pipe");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-pipe-{}",std::process::id());let model=dir.join("model");prepare_process(&exe,&model,&id).unwrap();let thread_id=id.clone();let (tx,rx)=std::sync::mpsc::channel();
 let worker=thread::spawn(move||{tx.send(finish_prepared(&model,&thread_id,&vec![1;1_900_000])).ok();});thread::sleep(Duration::from_millis(30));voice_cancel(id).unwrap();
 let result=rx.recv_timeout(Duration::from_secs(2));worker.join().unwrap();assert!(result.is_ok(),"write remained blocked after cancel");assert!(result.unwrap().is_err());assert!(PREWARM.lock().unwrap().is_none());
}

#[test]
fn shutdown_reaps_prepared_recognizer(){
 let _reset=ShutdownReset;SHUTTING_DOWN.store(false,std::sync::atomic::Ordering::SeqCst);let dir=temporary("warm-shutdown");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-shutdown-{}",std::process::id());prepare_process(&exe,&dir.join("model"),&id).unwrap();let pid=PREWARM.lock().unwrap().as_ref().unwrap().child.id() as libc::pid_t;shutdown();
 assert!(PREWARM.lock().unwrap().is_none());assert_eq!(unsafe{libc::kill(pid,0)},-1);assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));
}

#[test]
fn invalid_request_reaps_prepared_recognizer(){
 let dir=temporary("warm-invalid");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-invalid-{}",std::process::id());prepare_process(&exe,&dir.join("model"),&id).unwrap();
 assert!(validate_request("unknown",&id,b"bad").is_err());assert!(PREWARM.lock().unwrap().is_none());
}

#[test]
fn prepared_recognizer_has_deadline_even_when_pipe_is_blocked(){
 let dir=temporary("warm-timeout");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-timeout-{}",std::process::id());let model=dir.join("model");prepare_process(&exe,&model,&id).unwrap();let start=Instant::now();
 assert!(finish_prepared_until(&model,&id,&vec![1;1_900_000],start+Duration::from_millis(100)).is_err());assert!(start.elapsed()<Duration::from_secs(2));assert!(PREWARM.lock().unwrap().is_none());
}

#[test]
fn prepared_recognizer_has_deadline_after_audio_is_sent(){
 let dir=temporary("warm-wait-timeout");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\n/bin/cat >/dev/null\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-wait-timeout-{}",std::process::id());let model=dir.join("model");prepare_process(&exe,&model,&id).unwrap();let start=Instant::now();
 assert_eq!(finish_prepared_until(&model,&id,b"RIFF sample",start+Duration::from_millis(100)).unwrap_err(),VOICE_TIMEOUT);assert!(start.elapsed()<Duration::from_secs(2));assert!(PREWARM.lock().unwrap().is_none());
}

#[test]
fn fallback_timeout_reaps_process_and_removes_audio(){
 let dir=temporary("fallback-timeout");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("fallback-timeout-{}",std::process::id());let start=Instant::now();assert_eq!(transcribe_until(&exe,&dir.join("model"),&id,b"RIFF fake",start+Duration::from_millis(100)).unwrap_err(),VOICE_TIMEOUT);
 assert!(start.elapsed()<Duration::from_secs(2));assert!(ACTIVE.lock().unwrap().is_none());assert_no_recording_for(&id);
}

#[test]
fn timed_out_preparation_does_not_restart_fallback(){
 let dir=temporary("warm-no-fallback-after-timeout");let exe=dir.join("mock");let marker=dir.join("fallback");std::fs::write(&exe,format!("#!/bin/sh\ncase \" $* \" in *' -otxt '*) exec /bin/sleep 30;; *) printf launched > '{}';; esac\n",marker.display())).unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-no-fallback-{}",std::process::id());let model=dir.join("model");prepare_process(&exe,&model,&id).unwrap();
 assert_eq!(transcribe_with_prepared_until(&exe,&model,&id,&vec![1;1_900_000],Instant::now()+Duration::from_millis(100)).unwrap_err(),VOICE_TIMEOUT);
 assert!(!marker.exists());assert!(PREWARM.lock().unwrap().is_none());
}

#[test]
fn invalid_request_does_not_stop_another_prepared_recording(){
 let dir=temporary("warm-other-invalid");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\nexec /bin/sleep 30\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-other-{}",std::process::id());prepare_process(&exe,&dir.join("model"),&id).unwrap();assert!(validate_request("unknown","different-id",b"bad").is_err());
 assert!(PREWARM.lock().unwrap().as_ref().is_some_and(|entry|entry.id==id));voice_cancel(id).unwrap();
}

#[test]
fn failed_prepared_process_falls_back_without_restarting_cancelled_request(){
 let dir=temporary("warm-fallback");let exe=dir.join("mock");std::fs::write(&exe,b"#!/bin/sh\ncase \" $* \" in *' -otxt '*) exit 1;; *) printf fallback;; esac\n").unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("warm-fallback-{}",std::process::id());let model=dir.join("model");prepare_process(&exe,&model,&id).unwrap();
 assert_eq!(transcribe_with_prepared(&exe,&model,&id,b"RIFF fake").unwrap(),"fallback");assert!(PREWARM.lock().unwrap().is_none());
 let id=format!("warm-cancel-fallback-{}",std::process::id());prepare_process(&exe,&model,&id).unwrap();voice_cancel(id.clone()).unwrap();assert!(transcribe_with_prepared(&exe,&model,&id,b"RIFF fake").is_err());
}

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
fn quality_persists_and_rejects_invalid_values(){
 let dir=temporary("quality");assert_eq!(get_quality(&dir.0).unwrap(),"small");
 set_quality(&dir.0,"medium").unwrap();assert_eq!(get_quality(&dir.0).unwrap(),"medium");
 assert!(set_quality(&dir.0,"bad").is_err());assert_eq!(get_quality(&dir.0).unwrap(),"medium");
 set_quality(&dir.0,"base").unwrap();assert_eq!(get_quality(&dir.0).unwrap(),"base");
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
 assert_no_recording_for(&id);

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
 let audio=ACTIVE.lock().unwrap().as_ref().unwrap().2.clone();assert!(audio.exists());
 let start=Instant::now();voice_cancel(id.clone()).unwrap();assert!(worker.join().unwrap().is_err());
 assert!(start.elapsed()<Duration::from_secs(2),"cancel must not wait for the full decode");
 assert!(!audio.exists(),"the actual recording must be removed");
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
fn model_check_timeout_reaps_hash_process(){
 let dir=temporary("hash-timeout");let marker=dir.join("pid");let exe=dir.join("mock");std::fs::write(&exe,format!("#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",marker.display())).unwrap();std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let start=Instant::now();let result=bounded_hash(Command::new(&exe),Duration::from_millis(500));assert!(result.is_err());assert!(start.elapsed()<Duration::from_secs(3));
 let pid=std::fs::read_to_string(marker).unwrap().trim().parse::<libc::pid_t>().unwrap();assert_eq!(unsafe{libc::kill(pid,0)},-1);assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));
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
 let audio=ACTIVE.lock().unwrap().as_ref().unwrap().2.clone();assert!(audio.exists());
 let begin=Instant::now();shutdown();
 let cleaned_before_join=!audio.exists();let result=worker.join().unwrap();
 assert!(cleaned_before_join,"shutdown returned while actual audio still existed");assert!(result.is_err());
 assert!(begin.elapsed()<Duration::from_secs(2));assert!(ACTIVE.lock().unwrap().is_none());
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
 assert_no_recording_for(&id);
}

#[test]
fn recognizer_large_output_does_not_deadlock(){
 let dir=temporary("large-output");let exe=dir.join("mock");
 std::fs::write(&exe,b"#!/bin/sh\nexec /usr/bin/head -c 131072 /dev/zero\n").unwrap();
 std::fs::set_permissions(&exe,std::fs::Permissions::from_mode(0o700)).unwrap();
 let id=format!("large-output-{}",std::process::id());let thread_id=id.clone();let model=dir.join("model");
 let (send,recv)=std::sync::mpsc::channel();
 let worker=thread::spawn(move||{let result=transcribe(&exe,&model,&thread_id,b"RIFF fake");send.send(result).ok();});
 let result=recv.recv_timeout(Duration::from_secs(2));
 if result.is_err(){voice_cancel(id).unwrap();}
 worker.join().unwrap();
 assert!(result.is_ok(),"recognizer blocked while its output pipe filled");
 assert!(result.unwrap().unwrap_err().contains("слишком длинный"),"a very long output must produce the bounded-output error");
}

fn assert_no_recording_for(id:&str){
 let temp=std::env::temp_dir();let entry_name=format!("{}-{id}",std::process::id());
 for e in std::fs::read_dir(temp).unwrap().flatten(){
  if e.file_name().to_string_lossy().starts_with("atlas-voice-recordings-v1") {
   assert!(!e.path().join(&entry_name).join("audio.wav").exists(),"audio must be removed");
  }
 }
}

#[test]
fn shutdown_waits_for_recording_created_before_child_registration(){
 let _reset=ShutdownReset;SHUTTING_DOWN.store(false,std::sync::atomic::Ordering::SeqCst);
 let dir=temporary("shutdown-pending");let parent=dir.0.clone();let (send,recv)=std::sync::mpsc::channel();
 let worker=thread::spawn(move||{
  let _guard=VoiceWorker::start().unwrap();
  let (record,f)=super::super::voice_temp::create(&parent,std::process::id(),"pending").unwrap();drop(f);
  send.send(record.path.clone()).unwrap();
  let limit=Instant::now()+Duration::from_secs(1);
  while !SHUTTING_DOWN.load(std::sync::atomic::Ordering::SeqCst) && Instant::now()<limit {thread::sleep(Duration::from_millis(5));}
  thread::sleep(Duration::from_millis(50));drop(record);
 });
 let audio=recv.recv_timeout(Duration::from_secs(2)).unwrap();assert!(audio.exists());
 shutdown();let removed_before_join=!audio.exists();worker.join().unwrap();
 assert!(removed_before_join,"shutdown returned before the pending recording was removed");
}

#[test]
fn quality_reads_default_and_persists_for_a_new_process(){
 let dir=temporary("quality-persist");
 assert_eq!(get_quality(&dir.0).unwrap(),"small");
 set_quality(&dir.0,"medium").unwrap();assert_eq!(get_quality(&dir.0).unwrap(),"medium");
 let child=Command::new(std::env::current_exe().unwrap()).args(["--exact","voice::tests::quality_reader_process","--nocapture"])
  .env("ATLAS_VOICE_QUALITY_TEST_DIR",&dir.0).output().unwrap();
 assert!(child.status.success());assert!(String::from_utf8_lossy(&child.stdout).contains("VOICE_QUALITY=medium"));
 let files:Vec<_>=std::fs::read_dir(&dir.0).unwrap().map(|r|r.unwrap().file_name()).collect();
 assert_eq!(files,vec![std::ffi::OsString::from("voice-quality")],"selection must not download a model");
}

#[test]
fn quality_reader_process(){
 let Some(path)=std::env::var_os("ATLAS_VOICE_QUALITY_TEST_DIR")else{return};
 println!("VOICE_QUALITY={}",get_quality(std::path::Path::new(&path)).unwrap());
}

#[test]
fn invalid_quality_does_not_change_saved_choice(){
 let dir=temporary("quality-invalid");set_quality(&dir.0,"base").unwrap();
 assert!(set_quality(&dir.0,"unknown").is_err());assert_eq!(get_quality(&dir.0).unwrap(),"base");
 assert!(set_quality(&dir.0,"../medium").is_err());assert_eq!(get_quality(&dir.0).unwrap(),"base");
}

#[test]
fn failed_quality_write_preserves_existing_directory_and_no_temporary_file(){
 let dir=temporary("quality-failure");let blocked=dir.join("voice-quality");std::fs::create_dir(&blocked).unwrap();
 std::fs::write(blocked.join("keep"),b"keep").unwrap();assert!(set_quality(&dir.0,"medium").is_err());
 assert_eq!(std::fs::read(blocked.join("keep")).unwrap(),b"keep");
 let entries:Vec<_>=std::fs::read_dir(&dir.0).unwrap().map(|e|e.unwrap().file_name()).collect();
 assert_eq!(entries,vec![std::ffi::OsString::from("voice-quality")]);
}
