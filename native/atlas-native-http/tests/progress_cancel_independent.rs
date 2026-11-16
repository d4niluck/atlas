#[path="../src/model_queue.rs"] mod model_queue;
#[path="../src/semantic.rs"] mod semantic;
use semantic::SemanticService;
use std::{thread,time::{Duration,Instant}};
#[test] #[ignore = "requires local EmbeddingGemma model and llama-server"] fn cancel_stays_stopped_until_explicit_restart_and_keeps_index(){
 let p=std::env::temp_dir().join(format!("atlas-progress-independent-{}",std::process::id()));
 atlas_native_storage_contract::create_workspace(&p).unwrap();
 let baseline=atlas_native_storage_contract::add_file(&p,"baseline.txt",b"Astronomy research describes stars and galaxies across the universe.").unwrap();
 let s=SemanticService::start(&std::env::var("ATLAS_TEST_LLAMA").unwrap(),&std::env::var("ATLAS_TEST_EMBEDDING").unwrap()).unwrap();
 s.enqueue(&p,None).unwrap();let deadline=Instant::now()+Duration::from_secs(120);while Instant::now()<deadline && s.revision(&p).is_none(){thread::sleep(Duration::from_millis(100));}assert!(s.revision(&p).is_some());
 let current=std::fs::read(p.join(".atlas-semantic-native/current")).unwrap();
 let bytes = "Atlas test text with enough words to produce searchable fragments. ".repeat(16_000).into_bytes();
 let id=atlas_native_storage_contract::add_file(&p,"large.txt",&bytes).unwrap();
 s.enqueue(&p,Some(&id)).unwrap();
 let deadline=Instant::now()+Duration::from_secs(30);while Instant::now()<deadline && s.status(&p)["progress"].is_null() {thread::sleep(Duration::from_millis(20));}assert!(!s.status(&p)["progress"].is_null(),"large file never began processing");
 s.cancel(&p);let deadline=Instant::now()+Duration::from_secs(30);while Instant::now()<deadline && s.status(&p)["stopping"]==true{thread::sleep(Duration::from_millis(100));}
 assert_eq!(s.status(&p)["cancelled"],true);s.close(&p).unwrap();s.ensure(&p).unwrap();thread::sleep(Duration::from_millis(300));assert_eq!(s.status(&p)["cancelled"],true);assert_eq!(std::fs::read(p.join(".atlas-semantic-native/current")).unwrap(),current);
 atlas_native_storage_contract::delete_objects(&p,&[id]).unwrap();s.resume(&p).unwrap();let deadline=Instant::now()+Duration::from_secs(120);while Instant::now()<deadline && s.revision(&p).is_none(){thread::sleep(Duration::from_millis(100));}assert!(s.revision(&p).is_some());let found=s.search(&p,"stars and galaxies",None).unwrap();assert!(found.iter().any(|r|r["id"]==baseline),"baseline text should be searchable after resuming");s.close(&p).unwrap();std::fs::remove_dir_all(p).unwrap();
}
