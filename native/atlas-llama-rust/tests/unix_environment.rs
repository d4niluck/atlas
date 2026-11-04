#![cfg(all(unix,feature="test-hook"))]
use atlas_llama_probe::Encoder;
use std::{fs,io::{Read,Write},os::unix::net::UnixStream};
#[test] fn inherited_llama_environment_must_not_change_the_server() {
 let dir=std::env::temp_dir().join(format!("atlas-ipc-env-test-{}",std::process::id()));fs::create_dir(&dir).unwrap();
 let model=dir.join("model");fs::write(&model,b"dummy").unwrap();let key_file=dir.join("other-key");fs::write(&key_file,b"attacker-file-key").unwrap();
 std::env::set_var("LLAMA_API_KEY","attacker-env-key");std::env::set_var("LLAMA_ARG_API_KEY_FILE",&key_file);std::env::set_var("LLAMA_ARG_CONNECT","1");
 let start=Encoder::start(env!("CARGO_BIN_EXE_fake_model"),model.to_str().unwrap());
 std::env::remove_var("LLAMA_API_KEY");std::env::remove_var("LLAMA_ARG_API_KEY_FILE");std::env::remove_var("LLAMA_ARG_CONNECT");
 let encoder=start.expect("inherited LLAMA_ARG_CONNECT must not break startup");
 for key in ["attacker-env-key","attacker-file-key"] {
  let mut conn=UnixStream::connect(encoder.socket_path()).unwrap();write!(conn,"POST /v1/embeddings HTTP/1.0\r\nAuthorization: Bearer {key}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();let mut reply=String::new();conn.read_to_string(&mut reply).unwrap();assert!(reply.starts_with("HTTP/1.0 401"),"extra key was accepted: {reply}");
 }
 assert_eq!(encoder.encode("hello",true).unwrap().len(),512);drop(encoder);fs::remove_dir_all(dir).unwrap();
}
