mod chunk;
pub fn worker_chunks(text:&str)->Vec<serde_json::Value>{chunk::chunks(text,1000)}
use atlas_faiss_probe::Index;
use atlas_llama_probe::Encoder;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{io::Read,path::Path};

fn digest(path:&Path)->Result<String,String>{let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;let mut h=Sha256::new();let mut buf=[0u8;65536];loop{let n=file.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break}h.update(&buf[..n]);}Ok(format!("{:x}",h.finalize()))}

fn split(text:&str)->Result<Vec<Value>,String>{Ok(chunk::chunks(text,1000))}

fn input_hash(text:&str)->String{format!("{:x}",Sha256::digest(text.as_bytes()))}
fn reusable(dir:&Path,model:&Encoder)->std::collections::HashMap<String,[f32;512]>{
 let loaded=(||->Result<_,String>{let g=std::fs::read_to_string(dir.join("current")).map_err(|e|e.to_string())?;if !g.starts_with("gen-")||!g.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'){return Err("invalid generation".into())}let path=dir.join(g);let data:Value=serde_json::from_slice(&std::fs::read(path.join("metadata.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;if data["key"]!=model.identity()||data["index_sha256"]!=digest(&path.join("index.faiss"))?{return Err("cache incompatible".into())}let records=data["records"].as_array().ok_or("invalid records")?;let index=Index::load(&path.join("index.faiss")).map_err(str::to_string)?;if index.count()!=records.len() as i64{return Err("count mismatch".into())}let mut map=std::collections::HashMap::new();for (i,r) in records.iter().enumerate(){if let Some(k)=r["input_hash"].as_str(){map.insert(k.to_owned(),index.reconstruct(i as i64).map_err(str::to_string)?);}}Ok(map)})();loaded.unwrap_or_default()
}
fn encode_cached(model:&Encoder,text:&str,cache:&mut std::collections::HashMap<String,[f32;512]>,count:&mut usize,should_continue:&impl Fn()->bool)->Result<([f32;512],String),String>{let key=input_hash(text);if let Some(v)=cache.get(&key){return Ok((*v,key))}if !should_continue(){return Err("indexing cancelled".into())}let v:[f32;512]=model.encode(text,false)?.try_into().map_err(|_|"invalid dimensions")?;*count+=1;cache.insert(key.clone(),v);Ok((v,key))}
pub struct Source<'a> { pub id: &'a str, pub title: &'a str, pub body: &'a str, pub description: &'a str }
fn memory_limits()->(usize,usize){
 #[cfg(feature="test-hook")]
 {let vectors=std::env::var("ATLAS_TEST_PROTECTED_MAX_VECTORS").ok().and_then(|v|v.parse::<usize>().ok()).unwrap_or(16000).min(16000);let metadata=std::env::var("ATLAS_TEST_PROTECTED_MAX_METADATA").ok().and_then(|v|v.parse::<usize>().ok()).unwrap_or(32*1024*1024).min(32*1024*1024);(vectors,metadata)}
 #[cfg(not(feature="test-hook"))]
 {(16000,32*1024*1024)}
}
/// Keep FAISS and result metadata bounded while processing one source at a time.
pub struct MemoryBuilder {index:Index,records:Vec<Value>,cache:std::collections::HashMap<String,[f32;512]>,encoded_count:usize,metadata_bytes:usize}
impl MemoryBuilder {
 pub fn new(model:&Encoder,previous:Option<(&Index,&Value)>)->Result<Self,String>{let mut cache=std::collections::HashMap::new();if let Some((old,meta))=previous {if meta["key"]==model.identity(){if let Some(rows)=meta["records"].as_array(){if old.count()==rows.len() as i64&&rows.len()<=16000 {for (i,row) in rows.iter().enumerate(){if let Some(hash)=row["input_hash"].as_str(){cache.insert(hash.to_owned(),old.reconstruct(i as i64).map_err(str::to_string)?);}}}}}}Ok(Self{index:Index::new().map_err(str::to_string)?,records:Vec::new(),cache,encoded_count:0,metadata_bytes:0})}
 fn add(&mut self,model:&Encoder,text:&str,row:Value,should_continue:&impl Fn()->bool)->Result<(),String>{let(vector_limit,metadata_limit)=memory_limits();if self.records.len()>=vector_limit{return Err("Смысловой поиск ограничен 16 000 фрагментов".into())}let n=serde_json::to_vec(&row).map_err(|_|"invalid record")?.len();if self.metadata_bytes.saturating_add(n).saturating_add(80)>metadata_limit{return Err("Метаданные смыслового поиска превышают 32 МиБ".into())}let(vector,key)=encode_cached(model,text,&mut self.cache,&mut self.encoded_count,should_continue)?;let mut row=row;row["input_hash"]=json!(key);self.index.add(&[vector]).map_err(str::to_string)?;self.metadata_bytes+=n+80;self.records.push(row);Ok(())}
 pub fn push(&mut self,model:&Encoder,doc:&Source<'_>,should_continue:&impl Fn()->bool)->Result<(),String>{for(field,text)in [("body",doc.body),("description",doc.description)]{if text.trim().is_empty(){continue}for c in split(text)?{let fragment=c["text"].as_str().ok_or("missing text")?;self.add(model,&format!("{}\n{fragment}",doc.title),json!({"id":doc.id,"title":doc.title,"field":field,"text":fragment,"start":c["start"],"end":c["end"],"start_utf16":c["start_utf16"],"end_utf16":c["end_utf16"]}),should_continue)?;}}if doc.body.trim().is_empty()&&!doc.title.trim().is_empty(){self.add(model,doc.title,json!({"id":doc.id,"title":doc.title,"field":"title","text":doc.title,"start":null,"end":null}),should_continue)?;}Ok(())}
 pub fn push_image(&mut self,model:&Encoder,id:&str,title:&str,bytes:&[u8],mime:&str)->Result<(),String>{
  let(vector_limit,metadata_limit)=memory_limits();
  if self.records.len()>=vector_limit{return Err("Смысловой поиск ограничен 16 000 фрагментов".into())}
  let row=json!({"id":id,"title":title,"field":"image","text":"","start":null,"end":null});
  let n=serde_json::to_vec(&row).map_err(|_|"invalid record")?.len();
  if self.metadata_bytes.saturating_add(n).saturating_add(80)>metadata_limit{return Err("Метаданные смыслового поиска превышают 32 МиБ".into())}
  let key=format!("image:{:x}",Sha256::digest(bytes));let vector=if let Some(v)=self.cache.get(&key){*v}else{let v:[f32;512]=model.encode_image(bytes,mime)?.try_into().map_err(|_|"invalid dimensions")?;self.cache.insert(key.clone(),v);self.encoded_count+=1;v};
  let mut row=row;row["input_hash"]=json!(key);self.index.add(&[vector]).map_err(str::to_string)?;self.metadata_bytes+=n+80;self.records.push(row);Ok(())
 }
 pub fn finish(self,model:&Encoder)->Result<(Vec<u8>,Value),String>{let bytes=self.index.to_memory().map_err(str::to_string)?;let hash=format!("{:x}",Sha256::digest(&bytes));Ok((bytes,json!({"key":model.identity(),"binary_diagnostic":model.diagnostic(),"index_sha256":hash,"encoded_count":self.encoded_count,"records":self.records})))}
}
pub fn search_memory(model:&Encoder,index:&Index,data:&Value,q:&str,filter:Option<&[String]>,k:usize)->Result<Vec<Value>,String>{if data["key"]!=model.identity(){return Err("model key mismatch".into())}let records=data["records"].as_array().ok_or("invalid records")?;if index.count()!=records.len() as i64{return Err("index and records count mismatch".into())}let ids:Vec<i64>=records.iter().enumerate().filter(|(_,x)|filter.is_none_or(|allowed|allowed.iter().any(|id|x["id"].as_str()==Some(id)))).map(|(i,_)|i as i64).collect();let vector:[f32;512]=model.encode(q,true)?.try_into().map_err(|_|"invalid dimensions")?;index.search(&vector,&ids,k).map_err(str::to_string)?.into_iter().map(|(id,score)|{let mut row=records.get(id as usize).ok_or("index mismatch")?.clone();row["score"]=json!(score);Ok(row)}).collect()}
pub fn build(model:&Encoder,docs:&[(&str,&str,&str)],dir:&Path)->Result<(),String>{
 let converted:Vec<Source> = docs.iter().map(|&(id,title,body)|Source{id,title,body,description:""}).collect();
 build_sources(model,&converted,dir)
}
pub fn build_sources(model:&Encoder,docs:&[Source<'_>],dir:&Path)->Result<(),String>{
 let generation=prepare_sources(model,docs,dir)?;publish_sources(dir,&generation)
}
pub fn prepare_sources(model:&Encoder,docs:&[Source<'_>],dir:&Path)->Result<String,String>{prepare_sources_cancellable(model,docs,dir,||true)}
pub fn prepare_sources_cancellable(model:&Encoder,docs:&[Source<'_>],dir:&Path,should_continue:impl Fn()->bool)->Result<String,String>{prepare_sources_with_images(model,docs,std::iter::empty(),dir,should_continue)}
pub fn prepare_sources_with_images(model:&Encoder,docs:&[Source<'_>],images:impl Iterator<Item=Result<(String,String,Vec<u8>,String),String>>,dir:&Path,should_continue:impl Fn()->bool)->Result<String,String>{
 if !dir.is_dir(){std::fs::create_dir(dir).map_err(|e|e.to_string())?;}let mut index=Index::new().map_err(str::to_string)?;let mut meta=vec![];let mut cache=reusable(dir,model);let mut encoded_count=0usize;
 for doc in docs {
  for (field,text) in [("body",doc.body),("description",doc.description)] {
   if text.trim().is_empty(){continue}
   for c in split(text)? {let fragment=c["text"].as_str().ok_or("missing text")?;let (vector,key)=encode_cached(model,&format!("{}\n{fragment}",doc.title),&mut cache,&mut encoded_count,&should_continue)?;index.add(&[vector]).map_err(str::to_string)?;meta.push(json!({"id":doc.id,"title":doc.title,"field":field,"input_hash":key,"text":fragment,"start":c["start"],"end":c["end"],"start_utf16":c["start_utf16"],"end_utf16":c["end_utf16"]}));}
  }
  if doc.body.trim().is_empty() && !doc.title.trim().is_empty() {let (vector,key)=encode_cached(model,doc.title,&mut cache,&mut encoded_count,&should_continue)?;index.add(&[vector]).map_err(str::to_string)?;meta.push(json!({"id":doc.id,"title":doc.title,"field":"title","input_hash":key,"text":doc.title,"start":null,"end":null}));}
 }
 for image in images {let (id,title,bytes,mime)=match image{Ok(v)=>v,Err(e)=>{eprintln!("Atlas: image indexing skipped: {e}");continue}};if !should_continue(){return Err("indexing cancelled".into())}let key=format!("image:{:x}",Sha256::digest(&bytes));let vector=if let Some(v)=cache.get(&key){*v}else{let v:[f32;512]=match model.encode_image(&bytes,&mime).and_then(|v|v.try_into().map_err(|_|"invalid dimensions".into())){Ok(v)=>v,Err(e)=>{eprintln!("Atlas: image indexing skipped: {e}");continue}};cache.insert(key.clone(),v);encoded_count+=1;v};index.add(&[vector]).map_err(str::to_string)?;meta.push(json!({"id":id,"title":title,"field":"image","input_hash":key,"text":"","start":null,"end":null}));}
 if !should_continue(){return Err("indexing cancelled".into())}
 let generation=format!("gen-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());let stage=dir.join(&generation);std::fs::create_dir(&stage).map_err(|e|e.to_string())?;
 index.save(&stage.join("index.faiss")).map_err(str::to_string)?;std::fs::File::open(stage.join("index.faiss")).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;
 let index_digest=digest(&stage.join("index.faiss"))?;
 let meta_path=stage.join("metadata.json");std::fs::write(&meta_path,serde_json::to_vec(&json!({"key":model.identity(),"binary_diagnostic":model.diagnostic(),"index_sha256":index_digest,"encoded_count":encoded_count,"records":meta})).unwrap()).map_err(|e|e.to_string())?;std::fs::File::open(meta_path).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;
 std::fs::File::open(&stage).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;
 Ok(generation)
}
pub fn publish_sources(dir:&Path,generation:&str)->Result<(),String>{
 if !generation.starts_with("gen-")||!generation.chars().all(|c|c.is_ascii_alphanumeric()||c=='-')||!dir.join(generation).is_dir(){return Err("invalid generation".into())}
 let pointer=dir.join(format!("{generation}.tmp"));std::fs::write(&pointer,generation).map_err(|e|e.to_string())?;std::fs::File::open(&pointer).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;
 std::fs::rename(pointer,dir.join("current")).map_err(|e|e.to_string())?;std::fs::File::open(dir).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;Ok(())
}

pub fn search(model:&Encoder,dir:&Path,q:&str,filter:Option<&[&str]>,k:usize)->Result<Vec<Value>,String>{
 let generation=std::fs::read_to_string(dir.join("current")).map_err(|e|e.to_string())?;if !generation.starts_with("gen-")||!generation.chars().all(|c|c.is_ascii_alphanumeric()||c=='-'){return Err("invalid generation".into())}let dir=dir.join(generation);
 let data:Value=serde_json::from_slice(&std::fs::read(dir.join("metadata.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 if data["key"]!=model.identity() {return Err("model key mismatch; rebuild required".into())} let records=data["records"].as_array().ok_or("invalid records")?;
 let ids:Vec<i64>=records.iter().enumerate().filter(|(_,x)|filter.map_or(true,|allowed|allowed.contains(&x["id"].as_str().unwrap_or("")))).map(|(i,_)|i as i64).collect();
 if data["index_sha256"]!=digest(&dir.join("index.faiss"))?{return Err("index checksum mismatch".into())}
 let index=Index::load(&dir.join("index.faiss")).map_err(str::to_string)?;
 if index.count()!=records.len() as i64{return Err("index and records count mismatch".into())}
 let v:[f32;512]=model.encode(q,true)?.try_into().map_err(|_|"invalid dimensions")?;
 index.search(&v,&ids,k).map_err(str::to_string)?.into_iter().map(|(id,score)| {let mut x=records.get(id as usize).ok_or("index and records mismatch")?.clone();x["score"]=json!(score);Ok(x)}).collect()
}
#[cfg(test)] mod tests {use super::*;
 #[test] fn end_to_end_real_model(){if std::env::var_os("ATLAS_JOINED_REAL").is_none(){return} let dir=std::env::temp_dir().join(format!("atlas-joined-{}",std::process::id()));let model=Encoder::start("/private/tmp/atlas-llama-bin/llama-b12001/llama-server","/private/tmp/atlas-embeddinggemma-bf16.gguf").unwrap();let docs=[("a","Огород","# Помидоры\n\nПоливайте томаты под корень по утрам."),("b","Багаж","# Поездка\n\nПаспорт храните в ручной клади.")];build(&model,&docs,&dir).unwrap();std::fs::create_dir(dir.join("gen-interrupted")).unwrap();std::fs::write(dir.join("gen-interrupted/index.faiss"),b"incomplete").unwrap();drop(model);let model=Encoder::start("/private/tmp/atlas-llama-bin/llama-b12001/llama-server","/private/tmp/atlas-embeddinggemma-bf16.gguf").unwrap();let result=search(&model,&dir,"Как поливать томаты?",Some(&["b"]),1).unwrap();assert_eq!(result[0]["id"],"b");let result=search(&model,&dir,"Как поливать томаты?",Some(&["a","b"]),2).unwrap();assert_eq!(result[0]["id"],"a");for r in result {let doc=docs.iter().find(|d|d.0==r["id"].as_str().unwrap()).unwrap();let chars:Vec<char>=doc.2.chars().collect();let slice:String=chars[r["start"].as_u64().unwrap() as usize..r["end"].as_u64().unwrap() as usize].iter().collect();assert_eq!(slice,r["text"].as_str().unwrap());}let generation=std::fs::read_to_string(dir.join("current")).unwrap();let idxpath=dir.join(&generation).join("index.faiss");let original=std::fs::read(&idxpath).unwrap();let mut changed=original.clone();let n=changed.len();changed[n-1]^=1;std::fs::write(&idxpath,&changed).unwrap();assert!(search(&model,&dir,"тест",Some(&["a"]),1).unwrap_err().contains("checksum"));std::fs::write(&idxpath,&original).unwrap();let path=dir.join(generation).join("metadata.json");let mut data:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();let original_data=data.clone();data["records"].as_array_mut().unwrap().push(json!({"id":"c"}));std::fs::write(&path,serde_json::to_vec(&data).unwrap()).unwrap();assert!(search(&model,&dir,"тест",Some(&["a"]),1).unwrap_err().contains("count"));data=original_data;data["key"]=json!("wrong");std::fs::write(&path,serde_json::to_vec(&data).unwrap()).unwrap();assert!(search(&model,&dir,"тест",Some(&["a"]),1).is_err());std::fs::remove_dir_all(dir).unwrap();}
}
