#[path="../src/semantic.rs"] mod semantic;
use semantic::SemanticService;
use std::{thread,time::{Duration,Instant}};
#[test] #[ignore = "requires local EmbeddingGemma model and llama-server"] fn cancel_stays_stopped_until_explicit_restart_and_keeps_index(){
 let p=std::env::temp_dir().join(format!("atlas-progress-independent-{}",std::process::id()));
 atlas_native_storage_contract::create_workspace(&p).unwrap();
 let s=SemanticService::start("/private/tmp/atlas-llama-bin/llama-b12001/llama-server","/private/tmp/atlas-embeddinggemma-bf16.gguf").unwrap();
 s.enqueue(&p,None).unwrap();let deadline=Instant::now()+Duration::from_secs(120);while Instant::now()<deadline && s.revision(&p).is_none(){thread::sleep(Duration::from_millis(100));}assert!(s.revision(&p).is_some());
 let current=std::fs::read(p.join(".atlas-semantic-native/current")).unwrap();
 s.enqueue(&p,None).unwrap();s.cancel(&p);let deadline=Instant::now()+Duration::from_secs(30);while Instant::now()<deadline && s.status(&p)["stopping"]==true{thread::sleep(Duration::from_millis(100));}
 assert_eq!(s.status(&p)["cancelled"],true);s.close(&p).unwrap();s.ensure(&p).unwrap();thread::sleep(Duration::from_millis(300));assert_eq!(s.status(&p)["cancelled"],true);assert_eq!(std::fs::read(p.join(".atlas-semantic-native/current")).unwrap(),current);
 s.resume(&p).unwrap();let deadline=Instant::now()+Duration::from_secs(120);while Instant::now()<deadline && s.revision(&p).is_none(){thread::sleep(Duration::from_millis(100));}assert!(s.revision(&p).is_some());s.close(&p).unwrap();std::fs::remove_dir_all(p).unwrap();
}
