#![cfg(unix)]
#[path="../src/rag_model.rs"] mod rag_model;
use rag_model::RagModel;
use std::{fs,os::unix::fs::PermissionsExt,path::PathBuf};

struct Fixture { root:PathBuf, exe:PathBuf, model:PathBuf, args:PathBuf }
impl Fixture {
 fn new()->Self {
  let root=std::env::temp_dir().join(format!("atlas-ngram-lifecycle-{}",std::process::id()));
  fs::create_dir(&root).unwrap();let exe=root.join("server.py");let model=root.join("model");let args=root.join("args.json");
  let source=include_str!("../../atlas-llama-rust/tests/fixtures/model_env_fake.py").replace("args = sys.argv",&format!("with open({:?}, 'w') as f: json.dump({{'pid':os.getpid(),'args':sys.argv[1:]}}, f)\nargs = sys.argv",args.to_str().unwrap())).replace("os.environ['ATLAS_TEST_SOCKET_MARKER']",&format!("{:?}",root.join("socket").to_str().unwrap()));
  fs::write(&exe,source).unwrap();fs::set_permissions(&exe,fs::Permissions::from_mode(0o700)).unwrap();fs::write(&model,b"model").unwrap();Self{root,exe,model,args}
 }
 fn state(&self)->serde_json::Value{serde_json::from_slice(&fs::read(&self.args).unwrap()).unwrap()}
}
impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}
fn stopped(pid:i64){assert_eq!(unsafe{libc::kill(pid as i32,0)},-1,"old model process is still alive");assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));}
fn has_ngram(v:&serde_json::Value)->bool{v["args"].as_array().unwrap().iter().any(|a|a=="--spec-type")}
#[test] fn changing_ngram_restarts_only_the_llm_and_cpu_fallback_clears_it(){
 let f=Fixture::new();let exe=f.exe.to_str().unwrap();let path=f.model.to_str().unwrap();let rag=RagModel::lazy_with_ngram(exe.into(),path.into(),"metal".into(),false);
 assert!(rag.active_device().unwrap().is_none());{let _guard=rag.acquire().unwrap();}let old=f.state();let oldpid=old["pid"].as_i64().unwrap();assert!(!has_ngram(&old));
 rag.reconfigure_with_ngram(path,"metal",true,false).unwrap();{let _guard=rag.acquire().unwrap();}assert_eq!(f.state()["pid"],old["pid"]);
 rag.reconfigure_with_ngram(path,"metal",true,true).unwrap();stopped(oldpid);assert!(rag.active_device().unwrap().is_none());{let _guard=rag.acquire().unwrap();}let accelerated=f.state();let accelpid=accelerated["pid"].as_i64().unwrap();assert_ne!(oldpid,accelpid);assert!(has_ngram(&accelerated));
 let replacement=f.root.join("other-model");fs::write(&replacement,b"other model").unwrap();let replacement=replacement.to_str().unwrap();
 rag.reconfigure_with_ngram(replacement,"metal",true,true).unwrap();stopped(accelpid);{let _guard=rag.acquire().unwrap();}let switched=f.state();let switchedpid=switched["pid"].as_i64().unwrap();assert!(has_ngram(&switched));assert!(switched["args"].as_array().unwrap().iter().any(|a|a==replacement));
 rag.reconfigure_with_ngram(replacement,"cpu",true,true).unwrap();stopped(switchedpid);{let _guard=rag.acquire().unwrap();}let cpu=f.state();let cpupid=cpu["pid"].as_i64().unwrap();assert!(!has_ngram(&cpu));
 rag.reconfigure_with_ngram(replacement,"cpu",false,true).unwrap();stopped(cpupid);assert!(rag.acquire().is_err());
}
