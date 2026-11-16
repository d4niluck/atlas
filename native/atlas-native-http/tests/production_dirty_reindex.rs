#[path="../src/model_queue.rs"] mod model_queue;
#[path="../src/semantic.rs"] mod semantic;
use std::{time::{Duration,Instant},thread};
fn ready(s:&semantic::SemanticService,p:&std::path::Path){let until=Instant::now()+Duration::from_secs(120);while Instant::now()<until{if s.status(p)["available"]==true{return}thread::sleep(Duration::from_millis(50));}panic!("index did not become ready: {}",s.status(p));}
#[test]
fn dirty_agent_write_reindexes_and_survives_restart(){
 let root=std::env::temp_dir().join(format!("atlas-prod-dirty-{}",uuid::Uuid::new_v4()));atlas_native_storage_contract::create_workspace(&root).unwrap();
 let exe=std::env::var("ATLAS_TEST_LLAMA").unwrap();let model=std::env::var("ATLAS_TEST_EMBEDDING").unwrap();
 let service=semantic::SemanticService::start(&exe,&model).unwrap();service.ensure(&root).unwrap();ready(&service,&root);
 let marker=root.join(".atlas-agent-index-dirty");let note=atlas_native_storage_contract::agent_create_note_guarded(&root,"agent:test","request-one","Astronomy","The telescope captures distant stars",||{std::fs::write(&marker,b"pending")?;Ok(())}).unwrap();
 service.ensure(&root).unwrap();ready(&service,&root);assert!(!marker.exists(),"dirty marker was not cleared");assert!(service.search(&root,"telescope",None).unwrap().iter().any(|row|row["id"]==note["id"]));
 drop(service);let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","restart_child","--nocapture"]).env("ATLAS_DIRTY_TEST_ROOT",&root).env("ATLAS_DIRTY_TEST_ID",note["id"].as_str().unwrap()).status().unwrap();assert!(status.success(),"restart child failed");std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "run by parent process"]
fn restart_child(){let root=std::path::PathBuf::from(std::env::var("ATLAS_DIRTY_TEST_ROOT").unwrap());let id=std::env::var("ATLAS_DIRTY_TEST_ID").unwrap();let service=semantic::SemanticService::start(&std::env::var("ATLAS_TEST_LLAMA").unwrap(),&std::env::var("ATLAS_TEST_EMBEDDING").unwrap()).unwrap();service.ensure(&root).unwrap();ready(&service,&root);assert!(service.search(&root,"telescope",None).unwrap().iter().any(|row|row["id"]==id));}
