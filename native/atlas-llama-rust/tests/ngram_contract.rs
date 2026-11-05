#![cfg(unix)]
use atlas_llama_probe::{Encoder, Generator};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, sync::atomic::{AtomicUsize,Ordering}};
static NEXT:AtomicUsize=AtomicUsize::new(0);
struct Fixture { root: PathBuf, exe: PathBuf, model: PathBuf, args: PathBuf }
impl Fixture {
 fn new()->Self {
  let root=std::env::temp_dir().join(format!("atlas-ngram-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
  fs::create_dir(&root).unwrap();let exe=root.join("server.py");let model=root.join("model");let args=root.join("args.json");
  let source=include_str!("fixtures/model_env_fake.py").replace("args = sys.argv",&format!("with open({:?}, 'w') as f: json.dump(sys.argv[1:], f)\nargs = sys.argv",args.to_str().unwrap())).replace("os.environ['ATLAS_TEST_SOCKET_MARKER']",&format!("{:?}",root.join("socket").to_str().unwrap()));
  fs::write(&exe,source).unwrap();fs::set_permissions(&exe,fs::Permissions::from_mode(0o700)).unwrap();fs::write(&model,b"fake model").unwrap();Self{root,exe,model,args}
 }
 fn args(&self)->Vec<String>{serde_json::from_slice(&fs::read(&self.args).unwrap()).unwrap()}
 fn path(&self)->(&str,&str){(self.exe.to_str().unwrap(),self.model.to_str().unwrap())}
}
impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}
fn contains_flag(args:&[String],flag:&str,value:&str)->bool{args.windows(2).any(|w|w[0]==flag&&w[1]==value)}
#[test] fn metal_generator_only_uses_ngram_when_enabled(){
 let f=Fixture::new();let (exe,model)=f.path();
 let ordinary=Generator::start_device(exe,model,"metal").unwrap();assert!(!f.args().iter().any(|x|x=="--spec-type"));drop(ordinary);
 let accelerated=Generator::start_device_with_ngram(exe,model,"metal",true).unwrap();let args=f.args();assert!(contains_flag(&args,"--spec-type","ngram-simple"));assert!(contains_flag(&args,"--spec-ngram-simple-size-n","4"));assert!(contains_flag(&args,"--spec-ngram-simple-size-m","8"));drop(accelerated);
 let disabled=Generator::start_device_with_ngram(exe,model,"metal",false).unwrap();assert!(!f.args().iter().any(|x|x=="--spec-type"));drop(disabled);
}
#[test] fn cpu_and_embedding_never_use_ngram(){
 let f=Fixture::new();let (exe,model)=f.path();
 let generator=Generator::start_device_with_ngram(exe,model,"cpu",true).unwrap();assert!(!f.args().iter().any(|x|x=="--spec-type"));drop(generator);
 let encoder=Encoder::start_device(exe,model,"metal").unwrap();let args=f.args();assert!(args.iter().any(|x|x=="--embedding"));assert!(!args.iter().any(|x|x=="--spec-type"));drop(encoder);
}
