#![cfg(unix)]
use atlas_llama_probe::{Encoder,Generator};
use std::{fs,io::{Read,Write},os::unix::{fs::PermissionsExt,net::UnixStream},sync::{atomic::{AtomicUsize,Ordering},Mutex}};
static SERIAL:Mutex<()> = Mutex::new(());
static NEXT:AtomicUsize = AtomicUsize::new(0);
struct EnvGuard(&'static str,Option<std::ffi::OsString>);
impl EnvGuard {fn set(name:&'static str,value:impl AsRef<std::ffi::OsStr>)->Self{let old=std::env::var_os(name);std::env::set_var(name,value);Self(name,old)}}
impl Drop for EnvGuard {fn drop(&mut self){match &self.1 {Some(v)=>std::env::set_var(self.0,v),None=>std::env::remove_var(self.0)}}}

struct Fixture(std::path::PathBuf);
impl Fixture {
 fn new()->Self {let path=std::env::temp_dir().join(format!("atlas-env-test-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));fs::create_dir(&path).unwrap();let exe=path.join("server.py");fs::write(&exe,include_str!("fixtures/model_env_fake.py")).unwrap();fs::set_permissions(exe,fs::Permissions::from_mode(0o700)).unwrap();fs::write(path.join("model"),b"dummy").unwrap();Self(path)}
 fn start(&self)->Result<Encoder,String>{let _marker=EnvGuard::set("ATLAS_TEST_SOCKET_MARKER",self.0.join("socket"));Encoder::start(self.0.join("server.py").to_str().unwrap(),self.0.join("model").to_str().unwrap())}
}
impl Drop for Fixture {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
fn reject(socket:&std::path::Path,key:&str){let mut c=UnixStream::connect(socket).unwrap();write!(c,"POST /v1/embeddings HTTP/1.0\r\nAuthorization: Bearer {key}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();let mut s=String::new();c.read_to_string(&mut s).unwrap();assert!(s.starts_with("HTTP/1.0 401"),"unexpected status: {}",s.lines().next().unwrap_or(""));}
#[test] fn inherited_credentials_are_not_added_to_model(){let _guard=SERIAL.lock().unwrap();let f=Fixture::new();let key=f.0.join("other");fs::write(&key,b"attacker-file-key").unwrap();let _env_key=EnvGuard::set("LLAMA_API_KEY","attacker-env-key");let _file_key=EnvGuard::set("LLAMA_ARG_API_KEY_FILE",&key);let started=f.start();let model=started.unwrap();let socket=std::path::PathBuf::from(fs::read_to_string(f.0.join("socket")).unwrap());for key in ["attacker-env-key","attacker-file-key"]{reject(&socket,key);}assert_eq!(model.encode("text",true).unwrap().len(),512);drop(model);}
#[test] fn inherited_remote_tunnel_does_not_change_model(){let _guard=SERIAL.lock().unwrap();let f=Fixture::new();let _connect=EnvGuard::set("LLAMA_ARG_CONNECT","1");let started=f.start();let model=started.expect("untrusted environment must not configure model");assert_eq!(model.encode("text",true).unwrap().len(),512);}

#[test] fn inherited_settings_do_not_change_generator(){let _guard=SERIAL.lock().unwrap();let f=Fixture::new();let key=f.0.join("other");fs::write(&key,b"attacker-file-key").unwrap();let _env_key=EnvGuard::set("LLAMA_API_KEY","attacker-env-key");let _file_key=EnvGuard::set("LLAMA_ARG_API_KEY_FILE",&key);let _connect=EnvGuard::set("LLAMA_ARG_CONNECT","1");let _marker=EnvGuard::set("ATLAS_TEST_SOCKET_MARKER",f.0.join("socket"));let started=Generator::start_device(f.0.join("server.py").to_str().unwrap(),f.0.join("model").to_str().unwrap(),"cpu");let model=started.unwrap();let socket=std::path::PathBuf::from(fs::read_to_string(f.0.join("socket")).unwrap());for key in ["attacker-env-key","attacker-file-key"]{reject(&socket,key);}assert_eq!(model.answer("question",&[serde_json::json!({"text":"source"})]).unwrap(),"answer [1]");drop(model);}
