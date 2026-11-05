use atlas_joined_probe::{self as joined,overlay_gen,Source};use atlas_llama_probe::Encoder;use std::{env,fs,process::Command};
#[test]
fn worker_publish_and_cleanup(){
 let Some(dir)=env::var_os("ATLAS_TEST_RACE_CHILD_DIR") else{return};let generation=env::var("ATLAS_TEST_RACE_CHILD_NEW").unwrap();let old=env::var("ATLAS_TEST_RACE_CHILD_OLD").unwrap();let dir=std::path::Path::new(&dir);joined::publish_sources(dir,&generation).unwrap();fs::remove_dir_all(dir.join(old)).unwrap();
}
#[test]
fn prepared_delta_remains_valid_after_other_process_removes_original_base(){
 let root=env::temp_dir().join(format!("atlas-cross-process-{}",std::process::id()));fs::create_dir(&root).unwrap();let model_file=root.join("model");fs::write(&model_file,b"test-model").unwrap();env::set_var("ATLAS_TEST_FAKE_ENCODER_LOG",root.join("calls"));let encoder=Encoder::start(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/mock-embedding-server.py"),model_file.to_str().unwrap()).unwrap();let dir=root.join("index");
 let initial=[Source{id:"a",title:"A",body:"old A",description:""},Source{id:"b",title:"B",body:"keep B",description:""}];joined::build_sources(&encoder,&initial,&dir).unwrap();let original=fs::read_to_string(dir.join("current")).unwrap();
 let desired=[Source{id:"a",title:"A",body:"new A",description:""},Source{id:"b",title:"B",body:"keep B",description:""}];
 let staged=overlay_gen::prepare(&encoder,&desired,std::iter::empty(),&dir,||true,|_,_,_|{}).unwrap();let manifest:serde_json::Value=serde_json::from_slice(&fs::read(dir.join(&staged).join("overlay.json")).unwrap()).unwrap();assert_eq!(manifest["base_generation"],original);
 // Process A prepares a full compacted generation for exactly the same source snapshot.
 let new_base=joined::prepare_sources(&encoder,&desired,&dir).unwrap();let child=Command::new(env::current_exe().unwrap()).args(["--exact","worker_publish_and_cleanup","--nocapture"]).env("ATLAS_TEST_RACE_CHILD_DIR",&dir).env("ATLAS_TEST_RACE_CHILD_NEW",&new_base).env("ATLAS_TEST_RACE_CHILD_OLD",&original).status().unwrap();assert!(child.success());assert_eq!(fs::read_to_string(dir.join("current")).unwrap(),new_base);assert!(!dir.join(&original).exists());
 // The staged generation embeds its verified base, so it remains usable.
 overlay_gen::validate_prepared(&dir,&staged,encoder.identity()).unwrap();joined::publish_sources(&dir,&staged).unwrap();assert_eq!(fs::read_to_string(dir.join("current")).unwrap(),staged);let result=joined::search(&encoder,&dir,"query",None,10).unwrap();assert_eq!(result.len(),2);
 drop(encoder);fs::remove_dir_all(root).unwrap();
}
#[test]
fn worker_wait_for_workspace_lock_then_publish(){
 let Some(root)=env::var_os("ATLAS_TEST_READ_RACE_ROOT")else{return};let root=std::path::Path::new(&root);let dir=root.join("index");let generation=env::var("ATLAS_TEST_READ_RACE_NEW").unwrap();let old=env::var("ATLAS_TEST_READ_RACE_OLD").unwrap();fs::write(root.join("started"),b"started").unwrap();let _guard=atlas_native_storage_contract::workspace_lock(root).unwrap();fs::write(root.join("acquired"),b"acquired").unwrap();joined::publish_sources(&dir,&generation).unwrap();fs::remove_dir_all(dir.join(old)).unwrap();
}
#[test]
fn active_reader_holds_workspace_lock_until_search_finishes(){
 use std::time::{Duration,Instant};let root=env::temp_dir().join(format!("atlas-cross-read-{}",std::process::id()));fs::create_dir(&root).unwrap();let model_file=root.join("model");fs::write(&model_file,b"test-model").unwrap();env::set_var("ATLAS_TEST_FAKE_ENCODER_LOG",root.join("calls"));let encoder=Encoder::start(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/mock-embedding-server.py"),model_file.to_str().unwrap()).unwrap();let dir=root.join("index");let old_docs=[Source{id:"a",title:"A",body:"old content",description:""}];joined::build_sources(&encoder,&old_docs,&dir).unwrap();let original=fs::read_to_string(dir.join("current")).unwrap();let new_docs=[Source{id:"a",title:"A",body:"new content",description:""}];let new_base=joined::prepare_sources(&encoder,&new_docs,&dir).unwrap();
 let guard=atlas_native_storage_contract::workspace_lock(&root).unwrap();let mut child=Command::new(env::current_exe().unwrap()).args(["--exact","worker_wait_for_workspace_lock_then_publish","--nocapture"]).env("ATLAS_TEST_READ_RACE_ROOT",&root).env("ATLAS_TEST_READ_RACE_NEW",&new_base).env("ATLAS_TEST_READ_RACE_OLD",&original).spawn().unwrap();
 let deadline=Instant::now()+Duration::from_secs(10);while !root.join("started").exists()&&Instant::now()<deadline{std::thread::sleep(Duration::from_millis(10))}assert!(root.join("started").exists());std::thread::sleep(Duration::from_millis(100));assert!(!root.join("acquired").exists(),"writer acquired lock while reader active");
 let result=joined::search(&encoder,&dir,"query",None,10).unwrap();assert_eq!(result[0]["text"],"old content");assert!(dir.join(&original).exists());drop(guard);
 assert!(child.wait().unwrap().success());assert!(root.join("acquired").exists());assert!(!dir.join(&original).exists());let result=joined::search(&encoder,&dir,"query",None,10).unwrap();assert_eq!(result[0]["text"],"new content");
 drop(encoder);fs::remove_dir_all(root).unwrap();
}
