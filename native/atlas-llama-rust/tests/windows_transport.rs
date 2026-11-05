#![cfg(all(windows,feature="test-hook"))]
use std::{fs,io::{Read,Write},path::PathBuf};
use atlas_llama_probe::Encoder;
fn model_dir()->PathBuf {static N:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);let p=std::env::temp_dir().join(format!("atlas-win-ipc-contract-{}-{}",std::process::id(),N.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));fs::create_dir(&p).unwrap();p}
#[test]
fn windows_local_socket_auth_search_and_cleanup() {
 let d=model_dir(); let model=d.join("test.gguf");fs::write(&model,b"not a real model").unwrap();
 let exe=std::env::var("ATLAS_FAKE_MODEL").expect("ATLAS_FAKE_MODEL");
 let encoder=Encoder::start(&exe,model.to_str().unwrap()).unwrap();
 let socket=encoder.socket_path().to_owned();let runtime=socket.parent().unwrap().to_owned();
 let process=unsafe{OpenProcess(0x0010_0000,0,encoder.process_id())};assert!(!process.is_null());
 let mut connection=uds_windows::UnixStream::connect(&socket).unwrap();
 connection.write_all(b"POST /v1/embeddings HTTP/1.0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
 let mut reply=String::new();connection.read_to_string(&mut reply).unwrap();assert!(reply.starts_with("HTTP/1.0 401"),"{reply}");
 assert_eq!(encoder.encode("поиск",true).unwrap().len(),512);
 atlas_llama_probe::set_search_deadline(std::time::Instant::now()+std::time::Duration::from_millis(300));
 let started=std::time::Instant::now();assert!(encoder.encode("DELAY",true).is_err());assert!(started.elapsed()<std::time::Duration::from_secs(3));
 drop(encoder); assert_eq!(unsafe{WaitForSingleObject(process,5000)},0,"model process still running");unsafe{CloseHandle(process)}; assert!(!runtime.exists());fs::remove_dir_all(d).unwrap();
}

#[link(name="kernel32")] unsafe extern "system" {fn OpenProcess(access:u32,inherit:i32,pid:u32)->*mut std::ffi::c_void;fn WaitForSingleObject(handle:*mut std::ffi::c_void,milliseconds:u32)->u32;fn CloseHandle(handle:*mut std::ffi::c_void)->i32;}

#[test]
fn windows_generator_auth_answer_and_cleanup() {
 let d=model_dir();let model=d.join("test.gguf");fs::write(&model,b"not a real model").unwrap();
 let exe=std::env::var("ATLAS_FAKE_MODEL").expect("ATLAS_FAKE_MODEL");
 std::env::set_var("LLAMA_ARG_CONNECT","1");
 let start=atlas_llama_probe::Generator::start_device(&exe,model.to_str().unwrap(),"cpu");
 std::env::remove_var("LLAMA_ARG_CONNECT");let generator=start.unwrap();
 let runtime=generator.socket_path().parent().unwrap().to_owned();let process=unsafe{OpenProcess(0x0010_0000,0,generator.process_id())};assert!(!process.is_null());
 let result=generator.answer("question",&[serde_json::json!({"title":"test","text":"source"})]).unwrap();assert_eq!(result,"answer [1]");
 drop(generator);assert_eq!(unsafe{WaitForSingleObject(process,5000)},0);unsafe{CloseHandle(process)};assert!(!runtime.exists());fs::remove_dir_all(d).unwrap();
}

#[test]
fn startup_respects_search_deadline_when_health_stalls() {
 let d=model_dir();let model=d.join("test.gguf");fs::write(&model,b"dummy").unwrap();let marker=d.join("health-started");
 let exe=std::env::var("ATLAS_FAKE_MODEL").expect("ATLAS_FAKE_MODEL");std::env::set_var("ATLAS_FAKE_STALL_HEALTH",&marker);
 let start=std::time::Instant::now();atlas_llama_probe::set_search_deadline(start+std::time::Duration::from_secs(15));
 let result=Encoder::start(&exe,model.to_str().unwrap());std::env::remove_var("ATLAS_FAKE_STALL_HEALTH");
 assert!(result.is_err());assert!(marker.exists(),"health was never reached");assert!(start.elapsed()<std::time::Duration::from_secs(20));
 fs::remove_dir_all(d).unwrap();
}
#[test]
fn expired_search_deadline_interrupts_model_hashing() {
 let d=model_dir();let model=d.join("test.gguf");fs::write(&model,b"dummy").unwrap();
 let model_string=model.to_str().unwrap().to_owned();
 let check=std::thread::spawn(move || {atlas_llama_probe::set_search_deadline(std::time::Instant::now());atlas_llama_probe::Encoder::model_identity(&model_string,"cpu")});
 assert!(check.join().unwrap().is_err());fs::remove_dir_all(d).unwrap();
}

#[test]
fn inherited_llama_key_file_must_not_authorize_extra_key() {
 let d=model_dir();let model=d.join("test.gguf");fs::write(&model,b"dummy").unwrap();let key_file=d.join("external-key");fs::write(&key_file,b"attacker-key").unwrap();
 let exe=std::env::var("ATLAS_FAKE_MODEL").expect("ATLAS_FAKE_MODEL");std::env::set_var("LLAMA_ARG_API_KEY_FILE",&key_file);
 let started=Encoder::start(&exe,model.to_str().unwrap());std::env::remove_var("LLAMA_ARG_API_KEY_FILE");let encoder=started.unwrap();
 let mut connection=uds_windows::UnixStream::connect(encoder.socket_path()).unwrap();
 connection.write_all(b"POST /v1/embeddings HTTP/1.0\r\nAuthorization: Bearer attacker-key\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();let mut reply=String::new();connection.read_to_string(&mut reply).unwrap();assert!(reply.starts_with("HTTP/1.0 401"),"unexpected response: {reply}");
 drop(encoder);fs::remove_dir_all(d).unwrap();
}

#[test]
fn windows_streaming_answer_and_cancellation() {
 let d=model_dir();let model=d.join("test.gguf");fs::write(&model,b"dummy").unwrap();
 let exe=std::env::var("ATLAS_FAKE_MODEL").expect("ATLAS_FAKE_MODEL");
 let generator=atlas_llama_probe::Generator::start_device(&exe,model.to_str().unwrap(),"cpu").unwrap();
 let observed=std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));let target=observed.clone();
 let cancel=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
 atlas_llama_probe::set_stream_sink(std::sync::Arc::new(move |s|target.lock().unwrap().push(s.to_owned())),cancel.clone());
 let answer=generator.answer("question",&[serde_json::json!({"title":"test","text":"source"})]).unwrap();
 atlas_llama_probe::clear_stream_sink();assert_eq!(answer,"answer [1]");assert_eq!(observed.lock().unwrap().concat(),answer);
 drop(generator);
 std::env::set_var("ATLAS_FAKE_STALL_STREAM","1");let generator=atlas_llama_probe::Generator::start_device(&exe,model.to_str().unwrap(),"cpu").unwrap();std::env::remove_var("ATLAS_FAKE_STALL_STREAM");
 let trigger=cancel.clone();std::thread::spawn(move ||{std::thread::sleep(std::time::Duration::from_millis(500));trigger.store(true,std::sync::atomic::Ordering::Release);});
 atlas_llama_probe::set_stream_sink(std::sync::Arc::new(|_|{}),cancel);let start=std::time::Instant::now();let result=generator.answer("question",&[serde_json::json!({"text":"source"})]);atlas_llama_probe::clear_stream_sink();
 assert!(result.is_err());assert!(start.elapsed()<std::time::Duration::from_secs(4));drop(generator);fs::remove_dir_all(d).unwrap();
}
