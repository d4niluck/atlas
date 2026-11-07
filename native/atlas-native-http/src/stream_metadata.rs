//! Read large index metadata without retaining every fragment.
use serde::{Deserialize,Deserializer,de::{self,SeqAccess,Visitor}};
use sha2::{Digest,Sha256};
use std::{fmt,io::{self,Read}};
use serde_json::Value;

#[derive(Deserialize)]
pub(crate) struct Metadata {pub model:String,pub index_sha256:String,pub records:Records}
pub(crate) struct Records {pub first:Vec<Value>,pub count:usize}
impl<'de> Deserialize<'de> for Records {
 fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{struct RecordVisitor;
  impl<'de> Visitor<'de> for RecordVisitor {type Value=Records;
   fn expecting(&self,f:&mut fmt::Formatter)->fmt::Result{f.write_str("an array of index records")}
   fn visit_seq<A:SeqAccess<'de>>(self,mut seq:A)->Result<Records,A::Error>{let mut first=Vec::new();let mut count=0usize;while count<16000{match seq.next_element::<Value>()?{Some(v)=>{first.push(v);count+=1},None=>return Ok(Records{first,count})}}while seq.next_element::<de::IgnoredAny>()?.is_some(){count=count.checked_add(1).ok_or_else(||de::Error::custom("too many records"))?;}Ok(Records{first,count})}
  }d.deserialize_seq(RecordVisitor)
 }
}
struct HashReader<R>{inner:R,hash:Sha256}
impl<R:Read> Read for HashReader<R>{fn read(&mut self,b:&mut[u8])->io::Result<usize>{let n=self.inner.read(b)?;self.hash.update(&b[..n]);Ok(n)}}
pub(crate) fn read_checked<R:Read>(input:R,expected:&str,model:&str)->Result<Metadata,String>{let r=HashReader{inner:input,hash:Sha256::new()};let mut reader=io::BufReader::new(r);let meta:Metadata=serde_json::from_reader(&mut reader).map_err(|e|e.to_string())?;let actual=format!("{:x}",reader.into_inner().hash.finalize());if actual!=expected{return Err("metadata checksum mismatch".into())}if !atlas_llama_probe::index_keys_compatible(&meta.model,model){return Err("model mismatch".into())}Ok(meta)}
#[cfg(test)] mod tests{use super::*;
 #[test] fn bounded_and_checked(){let rows=(0..16001).map(|i|serde_json::json!({"field":"body","title":"T","text":format!("{i}{}","x".repeat(2100))})).collect::<Vec<_>>();let data=serde_json::to_vec(&serde_json::json!({"model":"m","index_sha256":"hash","records":rows})).unwrap();assert!(data.len()>32*1024*1024);let hash=format!("{:x}",Sha256::digest(&data));let parsed=read_checked(data.as_slice(),&hash,"m").unwrap();assert_eq!(parsed.records.count,16001);assert_eq!(parsed.records.first.len(),16000);assert_eq!(parsed.model,"m");assert!(read_checked(data.as_slice(),&hash,"other").is_err());assert!(read_checked(&data[..data.len()-1],&hash,"m").is_err());let mut corrupt=data.clone();let i=corrupt.len()/2;corrupt[i]^=1;assert!(read_checked(corrupt.as_slice(),&hash,"m").is_err());assert!(read_checked(data.as_slice(),"wrong","m").is_err());}
}
