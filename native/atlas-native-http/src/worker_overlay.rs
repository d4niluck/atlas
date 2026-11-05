use serde_json::Value;
type R<T>=Result<T,Box<dyn std::error::Error>>;
// The external worker receives one document at a time. Keep unchanged vectors in
// the immutable base and rebuild the small overlay once after the whole scan.
pub(crate) struct WorkerOverlayPlan {
 overlay: atlas_faiss_probe::overlay::FlatOverlay,
 old: std::collections::BTreeMap<String, std::sync::Arc<Vec<Value>>>,
 base_rows: std::collections::BTreeMap<String, std::sync::Arc<Vec<Value>>>,
 delta: std::collections::BTreeMap<String, std::sync::Arc<Vec<Value>>>,
 seen: std::collections::HashSet<String>,
 pub(crate) trash: std::collections::HashSet<String>,
 lookup:std::collections::HashMap<String,(bool,i64)>,
 pending: Option<(String,Vec<Value>,Vec<[f32;512]>,bool)>,
 hide_after:std::collections::HashSet<String>,
 updates: Vec<(String,Option<Vec<[f32;512]>>)>,
}
impl WorkerOverlayPlan {
 pub(crate) fn grouped(rows:&[Value])->R<std::collections::BTreeMap<String,std::sync::Arc<Vec<Value>>>>{let mut m=std::collections::BTreeMap::<String,Vec<Value>>::new();for row in rows {let id=row["id"].as_str().ok_or("invalid record id")?;m.entry(id.to_owned()).or_default().push(row.clone());}Ok(m.into_iter().map(|(id,v)|(id,std::sync::Arc::new(v))).collect())}
 pub(crate) fn new(base:atlas_faiss_probe::Index,base_rows:Vec<Value>,snapshot:Option<atlas_faiss_probe::overlay::DeltaSnapshot>,delta_rows:Vec<Value>)->R<Self>{
  let keys=base_rows.iter().map(|v|v["id"].as_str().map(str::to_owned).ok_or("invalid record id")).collect::<Result<Vec<_>,_>>()?;
  let mut old=Self::grouped(&base_rows)?;let original=old.clone();let delta=Self::grouped(&delta_rows)?;
  let mut lookup=std::collections::HashMap::new();for (i,row) in base_rows.iter().enumerate(){if let Some(hash)=row["input_hash"].as_str(){lookup.insert(hash.to_owned(),(false,i as i64));}}for (i,row) in delta_rows.iter().enumerate(){if let Some(hash)=row["input_hash"].as_str(){lookup.insert(hash.to_owned(),(true,i as i64));}}
  if let Some(ref snap)=snapshot {for id in snap.changed_keys.iter().chain(snap.removed.iter()){old.remove(id);}for (id,rows) in &delta{if !snap.changed_keys.contains(id){return Err("unexpected delta metadata".into())}old.insert(id.clone(),rows.clone());}}
  let overlay=match snapshot {Some(s)=>atlas_faiss_probe::overlay::FlatOverlay::from_snapshot(base,keys,s)?,None=>atlas_faiss_probe::overlay::FlatOverlay::new(base,keys)?};
  Ok(Self{overlay,old,base_rows:original,delta,seen:Default::default(),trash:Default::default(),lookup,pending:None,hide_after:Default::default(),updates:Vec::new()})
 }
 pub(crate) fn reuse(&self,key:&str)->R<Option<[f32;512]>>{match self.lookup.get(key){Some((true,i))=>Ok(Some(self.overlay.reconstruct_delta(*i)?)),Some((false,i))=>Ok(Some(self.overlay.reconstruct_base(*i)?)),None=>Ok(None)}}
 pub(crate) fn push(&mut self,row:Value,vector:[f32;512],hidden:bool)->R<()>{let id=row["id"].as_str().ok_or("invalid record id")?.to_owned();if hidden!=self.trash.contains(&id){return Err("invalid document visibility".into())}if self.pending.as_ref().is_some_and(|(old,_,_,_)|*old!=id){self.flush()?;}if self.seen.contains(&id){return Err("non-contiguous document".into())}let pending=self.pending.get_or_insert_with(||(id,Vec::new(),Vec::new(),hidden));if pending.3!=hidden{return Err("visibility changed".into())}pending.1.push(row);pending.2.push(vector);Ok(())}
 fn flush(&mut self)->R<()>{if let Some((id,rows,vectors,hidden))=self.pending.take(){self.seen.insert(id.clone());if hidden{self.hide_after.insert(id.clone());}else{self.overlay.revive(&id)?;}if self.old.get(&id).is_some_and(|old|old.as_ref()==&rows){}else{self.delta.insert(id.clone(),std::sync::Arc::new(rows));self.updates.push((id,Some(vectors)));}}Ok(())}

 pub(crate) fn prepare(mut self)->R<WorkerOverlayPrepared>{
  self.flush()?;for id in self.old.keys(){if !self.seen.contains(id){if self.trash.contains(id){self.overlay.hide(id)?;}else{self.delta.remove(id);self.updates.push((id.clone(),None));}}}
  self.overlay.apply_batch(self.updates)?;for id in self.hide_after{self.overlay.hide(&id)?;}let snapshot=self.overlay.snapshot_delta()?;let mut rows=Vec::with_capacity(snapshot.records.len());for (id,pos) in &snapshot.records{rows.push(self.delta.get(id).and_then(|v|v.get(*pos)).ok_or("missing delta metadata")?.clone())}
  let cursor=if snapshot.records.len()>1024||!snapshot.removed.is_empty(){Some(self.overlay.start_materialize(true)?)}else{None};
  Ok(WorkerOverlayPrepared{overlay:self.overlay,snapshot,rows,base_rows:self.base_rows,delta:self.delta,cursor,compact:None})
 }
 pub(crate) fn finish(self)->R<(atlas_faiss_probe::overlay::DeltaSnapshot,Vec<Value>,Option<(atlas_faiss_probe::Index,Vec<Value>,Vec<String>)>)>{let mut prepared=self.prepare()?;while !prepared.step()?{}prepared.finish()}
}
pub(crate) struct WorkerOverlayPrepared {
 overlay:atlas_faiss_probe::overlay::FlatOverlay,
 snapshot:atlas_faiss_probe::overlay::DeltaSnapshot,
 rows:Vec<Value>,
 base_rows:std::collections::BTreeMap<String,std::sync::Arc<Vec<Value>>>,
 delta:std::collections::BTreeMap<String,std::sync::Arc<Vec<Value>>>,
 cursor:Option<atlas_faiss_probe::overlay::MaterializeCursor>,
 compact:Option<(atlas_faiss_probe::Index,Vec<Value>,Vec<String>)>,
}
impl WorkerOverlayPrepared {
 pub(crate) fn delta_len(&self)->usize{self.snapshot.index.len()}
 pub(crate) fn metadata_len(&self)->R<usize>{Ok(serde_json::to_vec(&serde_json::json!({"changed_keys":self.snapshot.changed_keys,"records":self.snapshot.records,"removed":self.snapshot.removed,"hidden":self.snapshot.hidden,"rows":self.rows}))?.len())}
 pub(crate) fn force_compaction(&mut self)->R<()>{if self.cursor.is_none()&&self.compact.is_none(){self.cursor=Some(self.overlay.start_materialize(true)?);}Ok(())}
 pub(crate) fn step(&mut self)->R<bool>{
  let Some(cursor)=self.cursor.as_mut() else{return Ok(true)};
  if !cursor.step(&self.overlay,256)?{return Ok(false)}
  let cursor=self.cursor.take().ok_or("missing compact cursor")?;let(index,ids)=cursor.finish(&self.overlay)?;
  let mut rows=Vec::with_capacity(ids.len());for(id,pos)in ids{let map=if self.snapshot.changed_keys.contains(&id){&self.delta}else{&self.base_rows};rows.push(map.get(&id).and_then(|v|v.get(pos)).ok_or("missing compact metadata")?.clone());}
  self.compact=Some((index,rows,self.snapshot.hidden.clone()));Ok(true)
 }
 pub(crate) fn finish(self)->R<(atlas_faiss_probe::overlay::DeltaSnapshot,Vec<Value>,Option<(atlas_faiss_probe::Index,Vec<Value>,Vec<String>)>)>{if self.cursor.is_some(){return Err("compaction unfinished".into())}Ok((self.snapshot,self.rows,self.compact))}
}
