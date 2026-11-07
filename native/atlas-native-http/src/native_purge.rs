//! Physically compact records whose source objects no longer exist, without a model.
use super::*;
use std::collections::HashSet;
use atlas_faiss_probe::overlay::FlatOverlay;
use super::fd_index_store::Store;

// Caller holds the workspace lock and supplies a consistent snapshot of object IDs.
pub(super) fn compact(cache:NativeCache, existing:&HashSet<String>, old:&str)->R<Option<NativeCache>>{
 let store=Store::open(".atlas-semantic-native")?;
 if store.current()?!=old{return Err("native index changed during cleanup".into())}
 let active=store.open_gen(old)?;
 let old_base=if cache.snapshot.is_some(){
  let manifest:Value=serde_json::from_slice(&store.read(&active,"overlay.json",100_000_000)?)?;
  let name=manifest["base_generation"].as_str().filter(|s|valid_worker_generation(s)).ok_or("invalid native base")?;
  match store.open_gen(name){Ok(_)=>Some(name.to_owned()),Err(e) if e.downcast_ref::<io::Error>().is_some_and(|e|e.kind()==io::ErrorKind::NotFound)=>Some(old.to_owned()),Err(e)=>return Err(e)}
 }else{None};
 let missing:HashSet<String>=cache.base_rows.iter().chain(cache.delta_rows.iter()).map(|r|r["id"].as_str().map(str::to_owned).ok_or("invalid native record")).collect::<Result<HashSet<_>,_>>()?.into_iter().filter(|id|!existing.contains(id)).collect();
 if missing.is_empty(){store.cleanup(old,old_base.as_deref())?;return Ok(Some(cache))}
 let base_groups=WorkerOverlayPlan::grouped(&cache.base_rows)?;
 let delta_groups=WorkerOverlayPlan::grouped(&cache.delta_rows)?;
 let keys=cache.base_rows.iter().map(|r|r["id"].as_str().map(str::to_owned).ok_or("invalid native id")).collect::<Result<Vec<_>,_>>()?;
 let changed:HashSet<String>=cache.snapshot.as_ref().map(|s|s.changed_keys.iter().cloned().collect()).unwrap_or_default();
 let mut overlay=match cache.snapshot {Some(snapshot)=>FlatOverlay::from_snapshot_index(cache.base,keys,snapshot,cache.delta.ok_or("missing native delta")?)?,None=>FlatOverlay::new(cache.base,keys)?};
 overlay.apply_batch(missing.into_iter().map(|id|(id,None)).collect())?;
 let hidden=overlay.hidden_keys();let(index,layout)=overlay.materialize_including_hidden()?;
 let mut records=Vec::with_capacity(layout.len());
 for(id,pos) in layout {let rows=if changed.contains(&id){delta_groups.get(&id)}else{base_groups.get(&id)};records.push(rows.and_then(|r|r.get(pos)).ok_or("missing compacted native metadata")?.clone());}
 let serial=uuid::Uuid::new_v4().simple().to_string();let base_name=format!("gen-{serial}-base");let final_name=if hidden.is_empty(){base_name.clone()}else{format!("gen-{serial}")};
 let mut created_base=false;let mut created_final=false;let mut published=false;
 let result=(||->R<()> {
  let base=store.create_gen(&base_name)?;created_base=true;
  let index_sha=store.write_index(&base,&index)?;
  let meta=serde_json::to_vec(&json!({"key":cache.key,"index_sha256":index_sha,"encoded_count":0,"records":records}))?;store.write(&base,"metadata.json",&meta)?;base.sync_all()?;
  let final_dir=if !hidden.is_empty(){
   let dir=store.create_gen(&final_name)?;created_final=true;
   for name in ["metadata.json","index.faiss"]{store.copy_file(&base,&dir,name)?;}
   let ids=records.iter().map(|r|r["id"].as_str().map(str::to_owned).ok_or("invalid native id")).collect::<Result<Vec<_>,_>>()?;let mut hidden_overlay=FlatOverlay::new(index,ids)?;for id in hidden{hidden_overlay.hide(&id)?}let snapshot=hidden_overlay.snapshot_delta()?;
   store.write(&dir,"delta.faiss",&snapshot.index)?;
   let manifest=serde_json::to_vec(&json!({"key":cache.key,"base_generation":base_name,"base_metadata_sha256":hash_bytes(&meta),"base_index_sha256":index_sha,"delta_sha256":hash_bytes(&snapshot.index),"changed_keys":snapshot.changed_keys,"delta_records":snapshot.records,"removed":snapshot.removed,"hidden":snapshot.hidden,"delta_metadata":[],"encoded_count":0}))?;
   store.write(&dir,"overlay.json",&manifest)?;store.write(&dir,"overlay.sha256",hash_bytes(&manifest).as_bytes())?;dir.sync_all()?;Some(dir)
  }else{None};
  store.verify_gen(&base_name,&base)?;if let Some(dir)=&final_dir{store.verify_gen(&final_name,dir)?;}
  let publication=store.publish(&final_name,old);published=store.current().ok().is_some_and(|v|v==final_name);publication?;
  store.cleanup(&final_name,if final_name==base_name{None}else{Some(&base_name)})?;Ok(())
 })();
 if result.is_err()&&!published{if created_final{let _=store.remove_gen(&final_name);}if created_base{let _=store.remove_gen(&base_name);}}
 result.map(|_|None)
}
