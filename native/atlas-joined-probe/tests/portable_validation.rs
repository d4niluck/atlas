use atlas_joined_probe::overlay_gen::validate_prepared_compatible;
use serde_json::json;
use sha2::{Digest,Sha256};
use std::{fs, time::{SystemTime,UNIX_EPOCH}};
fn hash(bytes:&[u8])->String {format!("{:x}",Sha256::digest(bytes))}
#[test]
fn compatible_ids_do_not_make_mismatched_base_and_overlay_valid() {
 let root=std::env::temp_dir().join(format!("atlas-portable-validation-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
 let base=root.join("gen-base");let stage=root.join("gen-overlay");fs::create_dir_all(&base).unwrap();fs::create_dir(&stage).unwrap();
 let index=b"base index";let delta=b"delta index";fs::write(base.join("index.faiss"),index).unwrap();fs::write(stage.join("delta.faiss"),delta).unwrap();
 let meta=serde_json::to_vec(&json!({"key":"cpu","index_sha256":hash(index)})).unwrap();fs::write(base.join("metadata.json"),&meta).unwrap();
 let write_manifest=|key:&str| {let manifest=serde_json::to_vec(&json!({"key":key,"base_generation":"gen-base","base_metadata_sha256":hash(&meta),"base_index_sha256":hash(index),"delta_sha256":hash(delta)})).unwrap();fs::write(stage.join("overlay.json"),&manifest).unwrap();fs::write(stage.join("overlay.sha256"),hash(&manifest)).unwrap();};
 write_manifest("metal");assert!(validate_prepared_compatible(&root,"gen-overlay","cpu","metal").is_err());
 write_manifest("cpu");assert!(validate_prepared_compatible(&root,"gen-overlay","cpu","metal").is_ok());
 fs::remove_dir_all(root).unwrap();
}
