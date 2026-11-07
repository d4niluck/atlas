use crate::Index;
use std::{collections::{BTreeMap,HashMap,HashSet},sync::atomic::{AtomicU64,Ordering}};
static NEXT_OVERLAY_ID:AtomicU64=AtomicU64::new(1);

/// In-memory proof: immutable base plus a rebuilt index of changed objects.
/// Storage, generation commits and trash retention are intentionally not provided.
pub struct FlatOverlay {
 base:Index,
 owner:u64,
 epoch:u64,
 base_records:Vec<(String,usize)>,
 base_keys:HashSet<String>,
 changed:BTreeMap<String,Vec<[f32;512]>>,
 removed:HashSet<String>,
 hidden:HashSet<String>,
 delta:Index,
 delta_records:Vec<(String,usize)>,
}
/// Overlay snapshot contents. Callers must persist all four parts atomically.
pub struct DeltaSnapshot {
 pub index:Vec<u8>,
 pub changed_keys:Vec<String>,
 pub records:Vec<(String,usize)>,
 pub removed:Vec<String>,
 pub hidden:Vec<String>,
}
/// Incremental compaction. Its input must not change between steps.
pub struct MaterializeCursor {
 owner:u64, epoch:u64, include_hidden:bool, base_pos:usize, delta_pos:usize,
 index:Index, records:Vec<(String,usize)>, failed:bool,
}
impl MaterializeCursor {
 /// Scan up to `budget` source entries, yielding between calls.
 pub fn step(&mut self,source:&FlatOverlay,budget:usize)->Result<bool,String>{
  if self.failed||self.owner!=source.owner||self.epoch!=source.epoch{return Err("overlay changed during compaction".into())}
  if budget==0||budget>256{return Err("invalid compaction budget".into())}
  let mut vectors=Vec::with_capacity(budget);let mut records=Vec::with_capacity(budget);let mut scanned=0;
  while scanned<budget&&self.base_pos<source.base_records.len(){
   let i=self.base_pos;self.base_pos+=1;scanned+=1;let r=&source.base_records[i];
   if source.removed.contains(&r.0)||(!self.include_hidden&&source.hidden.contains(&r.0))||source.changed.contains_key(&r.0){continue}
   match source.base.reconstruct(i as i64){Ok(v)=>vectors.push(v),Err(e)=>{self.failed=true;return Err(e.into())}}records.push(r.clone());
  }
  while scanned<budget&&self.base_pos==source.base_records.len()&&self.delta_pos<source.delta_records.len(){
   let i=self.delta_pos;self.delta_pos+=1;scanned+=1;let r=&source.delta_records[i];
   if !self.include_hidden&&source.hidden.contains(&r.0){continue}
   match source.delta.reconstruct(i as i64){Ok(v)=>vectors.push(v),Err(e)=>{self.failed=true;return Err(e.into())}}records.push(r.clone());
  }
  if !vectors.is_empty(){if let Err(e)=self.index.add(&vectors){self.failed=true;return Err(e.into())}}self.records.extend(records);
  Ok(self.base_pos==source.base_records.len()&&self.delta_pos==source.delta_records.len())
 }
 pub fn finish(self,source:&FlatOverlay)->Result<(Index,Vec<(String,usize)>),String>{
  if self.failed||self.owner!=source.owner||self.epoch!=source.epoch{return Err("overlay changed during compaction".into())}
  if self.base_pos!=source.base_records.len()||self.delta_pos!=source.delta_records.len(){return Err("compaction incomplete".into())}
  Ok((self.index,self.records))
 }
}
impl FlatOverlay {
 pub fn new(base:Index,base_keys:Vec<String>)->Result<Self,String>{
  if base.count()!=base_keys.len() as i64{return Err("base count mismatch".into())}
  let mut counts=HashMap::<String,usize>::new();
  let base_records=base_keys.into_iter().map(|key|{let n=counts.entry(key.clone()).or_default();let pos=*n;*n+=1;(key,pos)}).collect();
  let base_keys=counts.into_keys().collect();
  Ok(Self{base,owner:NEXT_OVERLAY_ID.fetch_add(1,Ordering::Relaxed),epoch:0,base_records,base_keys,changed:BTreeMap::new(),removed:HashSet::new(),hidden:HashSet::new(),delta:Index::new().map_err(str::to_string)?,delta_records:Vec::new()})
 }
 pub fn replace(&mut self,key:&str,vectors:&[[f32;512]])->Result<(),String>{
  self.apply_batch(vec![(key.to_owned(),Some(vectors.to_vec()))])
 }
 pub fn remove(&mut self,key:&str)->Result<(),String>{self.purge(key)}
 /// Hide retains all vectors until the caller purges the expired trash entry.
 pub fn hide(&mut self,key:&str)->Result<(),String>{
  if !self.changed.contains_key(key)&&(!self.base_keys.contains(key)||self.removed.contains(key)){return Err("unknown key".into())}
  if self.hidden.insert(key.to_owned()){self.epoch=self.epoch.wrapping_add(1)}Ok(())
 }
 pub fn revive(&mut self,key:&str)->Result<(),String>{if self.hidden.remove(key){self.epoch=self.epoch.wrapping_add(1)}Ok(())}
 pub fn purge(&mut self,key:&str)->Result<(),String>{self.apply_batch(vec![(key.to_owned(),None)])}
 /// Read cached vectors by their physical index without copying the index.
 pub fn reconstruct_base(&self,id:i64)->Result<[f32;512],String>{self.base.reconstruct(id).map_err(str::to_string)}
 pub fn reconstruct_delta(&self,id:i64)->Result<[f32;512],String>{self.delta.reconstruct(id).map_err(str::to_string)}
 pub fn has_hidden(&self)->bool{!self.hidden.is_empty()}
 pub fn has_key(&self,key:&str)->bool{self.changed.contains_key(key)||(self.base_keys.contains(key)&&!self.removed.contains(key))}
 pub fn hidden_keys(&self)->Vec<String>{let mut keys=self.hidden.iter().cloned().collect::<Vec<_>>();keys.sort();keys}
 /// Stage a batch and construct its new index before mutating the live state.
 pub fn apply_batch(&mut self, updates:Vec<(String,Option<Vec<[f32;512]>>)>) -> Result<(),String>{
  if updates.iter().filter_map(|(_,v)|v.as_ref()).flatten().flatten().any(|v|!v.is_finite()){return Err("invalid vector".into())}
  let updates=updates.into_iter().collect::<BTreeMap<_,_>>();
  let mut view=self.changed.iter().map(|(k,v)|(k.as_str(),v.as_slice())).collect::<BTreeMap<_,_>>();
  for(key,value)in &updates{match value{Some(v)=>{view.insert(key.as_str(),v.as_slice());},None=>{view.remove(key.as_str());}}}
  let mut index=Index::new().map_err(str::to_string)?;let mut records=Vec::new();
  for(key,vectors)in view{index.add(vectors).map_err(str::to_string)?;records.extend((0..vectors.len()).map(|i|(key.to_owned(),i)));}
  for(key,value)in updates{self.hidden.remove(&key);match value{Some(v)=>{self.removed.remove(&key);self.changed.insert(key,v);},None=>{self.changed.remove(&key);if self.base_keys.contains(&key){self.removed.insert(key);}else{self.removed.remove(&key);}}}}
  self.delta=index;self.delta_records=records;self.epoch=self.epoch.wrapping_add(1);Ok(())
 }
 /// Materialize visible vectors for a legacy reader.
 pub fn materialize(&self)->Result<(Index,Vec<(String,usize)>),String>{self.materialize_cancellable(false,||true)}
 /// For compaction keep hidden vectors; the caller must reapply the hidden keys.
 pub fn materialize_including_hidden(&self)->Result<(Index,Vec<(String,usize)>),String>{self.materialize_cancellable(true,||true)}
 pub fn start_materialize(&self,include_hidden:bool)->Result<MaterializeCursor,String>{
  Ok(MaterializeCursor{owner:self.owner,epoch:self.epoch,include_hidden,base_pos:0,delta_pos:0,index:Index::new().map_err(str::to_string)?,records:Vec::new(),failed:false})
 }
 pub fn materialize_cancellable(&self,include_hidden:bool,should_continue:impl Fn()->bool)->Result<(Index,Vec<(String,usize)>),String>{
  let mut index=Index::new().map_err(str::to_string)?;let mut batch=Vec::with_capacity(256);let mut records=Vec::new();
  for (i,r) in self.base_records.iter().enumerate(){if !should_continue(){return Err("indexing cancelled".into())}if self.removed.contains(&r.0)||(!include_hidden&&self.hidden.contains(&r.0))||self.changed.contains_key(&r.0){continue}batch.push(self.base.reconstruct(i as i64).map_err(str::to_string)?);records.push(r.clone());if batch.len()==256{index.add(&batch).map_err(str::to_string)?;batch.clear();}}
  for (i,r) in self.delta_records.iter().enumerate(){if !should_continue(){return Err("indexing cancelled".into())}if !include_hidden&&self.hidden.contains(&r.0){continue}batch.push(self.delta.reconstruct(i as i64).map_err(str::to_string)?);records.push(r.clone());if batch.len()==256{index.add(&batch).map_err(str::to_string)?;batch.clear();}}
  if !should_continue(){return Err("indexing cancelled".into())}if !batch.is_empty(){index.add(&batch).map_err(str::to_string)?;}Ok((index,records))
 }
 pub fn snapshot_delta(&self)->Result<DeltaSnapshot,String>{
  let mut removed=self.removed.iter().cloned().collect::<Vec<_>>();removed.sort();let mut hidden=self.hidden.iter().cloned().collect::<Vec<_>>();hidden.sort();
  Ok(DeltaSnapshot{index:self.delta.to_memory().map_err(str::to_string)?,changed_keys:self.changed.keys().cloned().collect(),records:self.delta_records.clone(),removed,hidden})
 }
 pub fn from_snapshot(base:Index,base_keys:Vec<String>,snap:DeltaSnapshot)->Result<Self,String>{
  let index=Index::from_memory(&snap.index).map_err(str::to_string)?;
  Self::from_snapshot_index(base,base_keys,snap,index)
 }
 /// Load an already verified delta without retaining its serialized bytes in memory.
 pub fn from_snapshot_index(base:Index,base_keys:Vec<String>,snap:DeltaSnapshot,index:Index)->Result<Self,String>{
  let mut result=Self::new(base,base_keys)?;
  if index.count()!=snap.records.len() as i64{return Err("delta count mismatch".into())}
  let mut changed=BTreeMap::new();
  for key in snap.changed_keys {if changed.insert(key,Vec::new()).is_some(){return Err("duplicate changed key".into())}}
  for(i,(key,pos))in snap.records.iter().enumerate(){let values=changed.get_mut(key).ok_or("delta record missing key")?;if *pos!=values.len(){return Err("invalid delta position".into())}let vector=index.reconstruct(i as i64).map_err(str::to_string)?;if vector.iter().any(|v|!v.is_finite()){return Err("invalid vector".into())}values.push(vector)}
  let mut removed=HashSet::new();for key in snap.removed{if changed.contains_key(&key)||!removed.insert(key){return Err("invalid removed key".into())}}
  let mut hidden=HashSet::new();for key in snap.hidden{if removed.contains(&key)||(!changed.contains_key(&key)&&!result.base_keys.contains(&key))||!hidden.insert(key){return Err("invalid hidden key".into())}}
  let expected=changed.iter().flat_map(|(k,v)|(0..v.len()).map(move |i|(k.clone(),i))).collect::<Vec<_>>();
  if expected!=snap.records{return Err("invalid delta order".into())}
  result.changed=changed;result.removed=removed;result.hidden=hidden;result.delta=index;result.delta_records=snap.records;Ok(result)
 }
 // Expand only when the cutoff is tied: otherwise FAISS may hide a lexically
 // earlier key that belongs in the deterministic merged top-k.
 fn search_with_ties(index:&Index,q:&[f32;512],ids:&[i64],k:usize)->Result<Vec<(i64,f32)>,String>{
  if ids.is_empty()||k==0{return Ok(Vec::new())}
  let mut n=k.saturating_add(1).min(ids.len());
  loop{let rows=index.search(q,ids,n).map_err(str::to_string)?;
   if rows.len()<n || n==ids.len() || rows.len()<=k || rows[k-1].1!=rows[n-1].1{return Ok(rows)}
   n=n.saturating_mul(2).max(n+1).min(ids.len());
  }
 }
 pub fn search(&self,q:&[f32;512],allowed:Option<&[String]>,k:usize)->Result<Vec<(String,usize,f32)>,String>{
  let allowed_set=allowed.map(|a|a.iter().map(String::as_str).collect::<HashSet<_>>());
  let include=|key:&str|allowed_set.as_ref().is_none_or(|a|a.contains(key));
  let ids=self.base_records.iter().enumerate().filter(|(_,r)|include(&r.0)&&!self.removed.contains(&r.0)&&!self.hidden.contains(&r.0)&&!self.changed.contains_key(&r.0)).map(|(i,_)|i as i64).collect::<Vec<_>>();
  let dids=self.delta_records.iter().enumerate().filter(|(_,r)|include(&r.0)&&!self.hidden.contains(&r.0)).map(|(i,_)|i as i64).collect::<Vec<_>>();
  let mut results=Vec::new();for(id,score)in Self::search_with_ties(&self.base,q,&ids,k)?{let r=&self.base_records[id as usize];results.push((r.0.clone(),r.1,score));}
  for(id,score)in Self::search_with_ties(&self.delta,q,&dids,k)?{let r=&self.delta_records[id as usize];results.push((r.0.clone(),r.1,score));}
  results.sort_unstable_by(|a,b|b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));results.truncate(k);Ok(results)
 }
}
