use atlas_joined_probe::{overlay_gen,Source};
use atlas_llama_probe::Encoder;
use serde_json::Value;
use sha2::{Digest,Sha256};
use std::io::Read;

// Explicit opt-in: uses local model weights and never touches a user's workspace.
#[test]
fn rename_reuses_body_vectors_and_updates_search_metadata(){
 let (Ok(exe),Ok(model))=(std::env::var("ATLAS_TEST_LLAMA"),std::env::var("ATLAS_TEST_EMBEDDER")) else {return};
 let root=std::env::temp_dir().join(format!("atlas-rename-real-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
 std::fs::create_dir(&root).unwrap();
 let run=(||->Result<(),Box<dyn std::error::Error>>{
  let encoder=Encoder::start(&exe,&model)?;
  let body="Помидоры выращивают в теплице, регулярно поливают и защищают от заморозков. ".repeat(18);
  let source=|title|Source{id:"doc",title,body:&body,description:""};
  let first=overlay_gen::prepare_with_trash(&encoder,&[source("Огород")],std::iter::empty(),&[],&root,||true,|_,_,_|{})?;
  atlas_joined_probe::publish_sources(&root,&first.generation)?;
  let old=overlay_gen::search(&encoder,&root,"Как выращивать помидоры?",None,8)?;
  assert!(!old.is_empty());assert_eq!(old[0]["title"],"Огород");
  let next=overlay_gen::prepare_with_trash(&encoder,&[source("Теплица")],std::iter::empty(),&[],&root,||true,|_,_,_|{})?;
  assert!(next.owned);let manifest:Value=serde_json::from_slice(&std::fs::read(root.join(&next.generation).join("overlay.json"))?)?;
  assert_eq!(manifest["encoded_count"],0,"unchanged text was encoded again");
  atlas_joined_probe::publish_sources(&root,&next.generation)?;
  let new=overlay_gen::search(&encoder,&root,"Как выращивать помидоры?",None,8)?;
  assert_eq!(old.len(),new.len());assert_eq!(new[0]["title"],"Теплица");
  for (a,b) in old.iter().zip(&new){assert_eq!(a["text"],b["text"]);assert_eq!(a["score"],b["score"])}
  // Empty notes and unsupported files must publish an empty, searchable index.
  let empty_root=root.join("empty");std::fs::create_dir(&empty_root)?;
  let empty=[Source{id:"note",title:"Пустая заметка",body:"",description:""},Source{id:"file",title:"archive.bin",body:"",description:""}];
  let prepared=overlay_gen::prepare_with_trash(&encoder,&empty,std::iter::empty(),&[],&empty_root,||true,|_,_,_|{})?;
  let meta:Value=serde_json::from_slice(&std::fs::read(empty_root.join(&prepared.generation).join("metadata.json"))?)?;
  assert_eq!(meta["encoded_count"],0);assert!(meta["records"].as_array().unwrap().is_empty());
  atlas_joined_probe::publish_sources(&empty_root,&prepared.generation)?;
  assert!(overlay_gen::search(&encoder,&empty_root,"Пустая заметка",None,8)?.is_empty());
  Ok(())
 })();
 std::fs::remove_dir_all(&root).unwrap();run.unwrap();
}

#[test]
fn legacy_index_is_rejected_and_rebuilt_without_publishing_cancelled_work(){
 let (Ok(exe),Ok(model))=(std::env::var("ATLAS_TEST_LLAMA"),std::env::var("ATLAS_TEST_EMBEDDER")) else {return};
 let root=std::env::temp_dir().join(format!("atlas-migration-real-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
 std::fs::create_dir(&root).unwrap();
 let run=(||->Result<(),Box<dyn std::error::Error>>{
  let encoder=Encoder::start(&exe,&model)?;let title="Огород";let body="Помидоры выращивают в теплице и защищают от заморозков.";
  let source=Source{id:"doc",title,body,description:""};
  let mut h=Sha256::new();let mut model_file=std::fs::File::open(&model)?;let mut buffer=[0;65536];loop{let n=model_file.read(&mut buffer)?;if n==0{break}h.update(&buffer[..n]);}
  h.update(b"atlas-experiment-v1;chunk=1000;dim=512;l2;query=task: search result | query: ;document=title: none | text: ;cpu;ctx=2048");let old_key=format!("{:x}",h.finalize());
  let old_input=format!("{title}\n{body}");let vector:Vec<f32>=encoder.encode(&old_input,false)?;let vector:[f32;512]=vector.try_into().map_err(|_|"invalid vector")?;
  let mut old_index=atlas_faiss_probe::Index::new()?;old_index.add(&[vector])?;
  let gen=root.join("gen-legacy");std::fs::create_dir(&gen)?;let path=gen.join("index.faiss");old_index.save(&path)?;
  let file_hash=format!("{:x}",Sha256::digest(std::fs::read(path)?));let input_hash=format!("{:x}",Sha256::digest(old_input.as_bytes()));
  let metadata=serde_json::json!({"key":old_key,"index_sha256":file_hash,"records":[{"id":"doc","title":title,"field":"body","text":body,"input_hash":input_hash,"start":0,"end":body.chars().count()}]});
  std::fs::write(gen.join("metadata.json"),serde_json::to_vec(&metadata)?)?;std::fs::write(root.join("current"),"gen-legacy")?;
  assert!(overlay_gen::search(&encoder,&root,"помидоры",None,8).is_err());
  assert!(overlay_gen::prepare_with_trash(&encoder,&[Source{..source}],std::iter::empty(),&[],&root,||false,|_,_,_|{}).is_err());
  assert_eq!(std::fs::read_to_string(root.join("current"))?,"gen-legacy");
  let next=overlay_gen::prepare_with_trash(&encoder,&[source],std::iter::empty(),&[],&root,||true,|_,_,_|{})?;
  assert_ne!(next.generation,"gen-legacy");let meta:Value=serde_json::from_slice(&std::fs::read(root.join(&next.generation).join("metadata.json"))?)?;
  assert_eq!(meta["key"],encoder.identity());assert_eq!(meta["encoded_count"],1);assert_ne!(meta["records"][0]["input_hash"],input_hash);
  atlas_joined_probe::publish_sources(&root,&next.generation)?;let rows=overlay_gen::search(&encoder,&root,"помидоры",None,8)?;assert_eq!(rows[0]["id"],"doc");
  Ok(())
 })();
 std::fs::remove_dir_all(&root).unwrap();run.unwrap();
}
