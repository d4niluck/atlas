use atlas_sqlcipher_proof::Vault;
use atlas_faiss_probe::Index;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::fs;
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn blob(x:f32)->Vec<u8>{let mut idx=Index::new().unwrap();let mut v=[0f32;512];v.fill(x);idx.add(&[v]).unwrap();idx.to_memory().unwrap()}
fn base_meta(key:&str,bytes:&[u8])->Value{json!({"key":key,"index_sha256":hash(bytes),"records":[{"id":"base"}]})}
fn overlay_meta(key:&str,base:&[u8],delta:&[u8],marker:&str)->Value{json!({"key":key,"base_index_sha256":hash(base),"delta_sha256":hash(delta),"marker":marker})}
#[test]
fn encrypted_overlay_is_atomic_model_bound_and_removed_when_base_changes(){
 let root=std::env::temp_dir().join(format!("atlas-protected-overlay-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
 let mut vault=Vault::create(&root.join("workspace.atlas"),"temporary-password").unwrap();let secret="ATLAS_ENCRYPTED_OVERLAY_MARKER_26ca49a712";
 let base=blob(0.25);let base_m=base_meta("model-A",&base);vault.save_semantic(&base,&base_m).unwrap();assert!(vault.load_semantic_overlay().unwrap().is_none());
 let delta=blob(f32::from_bits(0x3f765432));let meta=overlay_meta("model-A",&base,&delta,secret);
 vault.save_semantic_overlay_guarded(&delta,&meta,||Ok(())).unwrap();assert_eq!(vault.load_semantic_overlay().unwrap(),Some((delta.clone(),meta.clone())));
 let (_,loaded_base)=vault.load_semantic().unwrap();assert_eq!(loaded_base,base_m);
 let modified=blob(0.75);let modified_meta=overlay_meta("model-A",&base,&modified,secret);
 assert!(vault.save_semantic_overlay_guarded(&modified,&modified_meta,||Err("cancelled".into())).is_err());
 assert_eq!(vault.load_semantic_overlay().unwrap(),Some((delta.clone(),meta.clone())));
 for invalid in [overlay_meta("model-B",&base,&delta,secret),overlay_meta("model-A",&modified,&delta,secret),overlay_meta("model-A",&base,&modified,secret)] {
  assert!(vault.save_semantic_overlay_guarded(&delta,&invalid,||Ok(())).is_err());
  assert_eq!(vault.load_semantic_overlay().unwrap(),Some((delta.clone(),meta.clone())));
 }
 let next_base=blob(0.5);let next_meta=base_meta("model-B",&next_base);
 assert!(vault.save_semantic_guarded(&next_base,&next_meta,||Err("stop".into())).is_err());assert_eq!(vault.load_semantic_overlay().unwrap(),Some((delta.clone(),meta.clone())));
 for entry in fs::read_dir(&root).unwrap(){let path=entry.unwrap().path();if path.is_file(){let contents=fs::read(&path).unwrap();assert!(!contents.windows(secret.len()).any(|w|w==secret.as_bytes()),"plaintext metadata leaked");let pattern=0x3f765432u32.to_le_bytes().repeat(16);assert!(!contents.windows(pattern.len()).any(|w|w==pattern),"plaintext vector leaked");}}
 vault.save_semantic(&next_base,&next_meta).unwrap();assert!(vault.load_semantic_overlay().unwrap().is_none());assert_eq!(vault.load_semantic().unwrap().1,next_meta);
 drop(vault);let reopened=Vault::open(&root.join("workspace.atlas"),"temporary-password").unwrap();assert!(reopened.load_semantic_overlay().unwrap().is_none());assert_eq!(reopened.load_semantic().unwrap().1,next_meta);drop(reopened);fs::remove_dir_all(root).unwrap();
}
#[test]
fn encrypted_compaction_atomically_replaces_base_and_delta(){
 let root=std::env::temp_dir().join(format!("atlas-encrypted-compact-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let mut vault=Vault::create(&root.join("workspace.atlas"),"temporary-password").unwrap();let secret="ATLAS_ENCRYPTED_COMPACT_MARKER_899fed714";
 let base=blob(0.11);let meta=base_meta("model-A",&base);vault.save_semantic(&base,&meta).unwrap();let delta=blob(0.22);let dm=overlay_meta("model-A",&base,&delta,secret);vault.save_semantic_overlay_guarded(&delta,&dm,||Ok(())).unwrap();
 let next=blob(0.33);let next_meta=base_meta("model-A",&next);let next_delta=blob(f32::from_bits(0x3f765432));let next_dm=overlay_meta("model-A",&next,&next_delta,secret);
 assert!(vault.save_semantic_compacted_guarded(&next,&next_meta,&next_delta,&next_dm,||Err("stop".into())).is_err());assert_eq!(vault.load_semantic().unwrap().1,meta);assert_eq!(vault.load_semantic_overlay().unwrap(),Some((delta.clone(),dm.clone())));
 let wrong_model=overlay_meta("model-B",&next,&next_delta,secret);let wrong_base=overlay_meta("model-A",&base,&next_delta,secret);let wrong_delta=overlay_meta("model-A",&next,&delta,secret);
 for bad in [wrong_model,wrong_base,wrong_delta]{assert!(vault.save_semantic_compacted_guarded(&next,&next_meta,&next_delta,&bad,||Ok(())).is_err());assert_eq!(vault.load_semantic().unwrap().1,meta);assert_eq!(vault.load_semantic_overlay().unwrap(),Some((delta.clone(),dm.clone())));}
 vault.save_semantic_compacted_guarded(&next,&next_meta,&next_delta,&next_dm,||Ok(())).unwrap();assert_eq!(vault.load_semantic().unwrap().1,next_meta);assert_eq!(vault.load_semantic_overlay().unwrap(),Some((next_delta,next_dm)));
 for entry in fs::read_dir(&root).unwrap(){let path=entry.unwrap().path();if path.is_file(){let contents=fs::read(path).unwrap();assert!(!contents.windows(secret.len()).any(|w|w==secret.as_bytes()));let pattern=0x3f765432u32.to_le_bytes().repeat(16);assert!(!contents.windows(pattern.len()).any(|w|w==pattern));}}
 drop(vault);let reopened=Vault::open(&root.join("workspace.atlas"),"temporary-password").unwrap();assert_eq!(reopened.load_semantic().unwrap().1,next_meta);assert!(reopened.load_semantic_overlay().unwrap().is_some());drop(reopened);fs::remove_dir_all(root).unwrap();
}
