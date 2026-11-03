use std::{ffi::{c_void,CString},path::Path};
unsafe extern "C" {fn af_memory_write(p:*mut c_void)->*mut c_void;fn af_memory_size(w:*mut c_void)->i64;fn af_memory_copy(w:*mut c_void,out:*mut u8);fn af_memory_free(w:*mut c_void);fn af_memory_read(data:*const u8,n:i64)->*mut c_void;fn af_create()->*mut c_void;fn af_free(p:*mut c_void);fn af_count(p:*mut c_void)->i64;fn af_reconstruct(p:*mut c_void,id:i64,out:*mut f32)->i32;fn af_add(p:*mut c_void,v:*const f32,n:i64)->i32;fn af_search(p:*mut c_void,q:*const f32,ids:*const i64,n:i64,k:i64,scores:*mut f32,out:*mut i64)->i32;fn af_save(p:*mut c_void,path:*const std::ffi::c_char)->i32;fn af_load(path:*const std::ffi::c_char)->*mut c_void;}
pub struct Index(*mut c_void);
impl Drop for Index {fn drop(&mut self){unsafe{af_free(self.0)}}}
impl Index {
 pub fn to_memory(&self)->Result<Vec<u8>,&'static str>{let w=unsafe{af_memory_write(self.0)};if w.is_null(){return Err("serialize failed")}let n=unsafe{af_memory_size(w)};if !(0..=134217728).contains(&n){unsafe{af_memory_free(w)};return Err("index too large")}let mut out=vec![0u8;n as usize];unsafe{af_memory_copy(w,out.as_mut_ptr());af_memory_free(w)};Ok(out)}
 pub fn from_memory(data:&[u8])->Result<Self,&'static str>{if data.len()>134217728{return Err("index too large")}let p=unsafe{af_memory_read(data.as_ptr(),data.len() as i64)};if p.is_null(){Err("deserialize failed")}else{Ok(Self(p))}}
 pub fn count(&self)->i64{unsafe{af_count(self.0)}}
 pub fn new()->Result<Self,&'static str>{let p=unsafe{af_create()};if p.is_null(){Err("create failed")}else{Ok(Self(p))}}
 pub fn reconstruct(&self,id:i64)->Result<[f32;512],&'static str>{let mut out=[0.;512];if unsafe{af_reconstruct(self.0,id,out.as_mut_ptr())}!=0{return Err("reconstruct failed")}Ok(out)}
 pub fn add(&mut self,v:&[[f32;512]])->Result<(),&'static str>{if unsafe{af_add(self.0,v.as_ptr().cast(),v.len() as i64)}!=0 {Err("add failed")}else{Ok(())}}
 pub fn search(&self,q:&[f32;512],ids:&[i64],k:usize)->Result<Vec<(i64,f32)>,&'static str>{if k==0||ids.is_empty(){return Ok(vec![])} let mut scores=vec![0.;k];let mut out=vec![-1;k];if unsafe{af_search(self.0,q.as_ptr(),ids.as_ptr(),ids.len() as i64,k as i64,scores.as_mut_ptr(),out.as_mut_ptr())}!=0{return Err("search failed")} Ok(out.into_iter().zip(scores).filter(|(id,_)|*id>=0).collect())}
 pub fn save(&self,path:&Path)->Result<(),&'static str>{let s=CString::new(path.to_str().ok_or("invalid path")?).map_err(|_|"invalid path")?;if unsafe{af_save(self.0,s.as_ptr())}!=0{Err("save failed")}else{Ok(())}}
 pub fn load(path:&Path)->Result<Self,&'static str>{let s=CString::new(path.to_str().ok_or("invalid path")?).map_err(|_|"invalid path")?;let p=unsafe{af_load(s.as_ptr())};if p.is_null(){Err("load failed")}else{Ok(Self(p))}}
}
#[cfg(test)] mod tests {use super::*;
 #[test] fn roundtrip_and_filter_before_topk(){let mut idx=Index::new().unwrap();let mut a=[0.;512];a[0]=1.;let mut b=[0.;512];b[0]=0.8;b[1]=0.6;let mut c=[0.;512];c[1]=1.;idx.add(&[a,b,c]).unwrap();assert_eq!(idx.search(&a,&[1,2],1).unwrap()[0].0,1);assert!(idx.search(&a,&[],1).unwrap().is_empty());let path=std::env::temp_dir().join(format!("atlas-faiss-probe-{}",std::process::id()));idx.save(&path).unwrap();let restored=Index::load(&path).unwrap();assert_eq!(restored.search(&a,&[0,1,2],2).unwrap().iter().map(|x|x.0).collect::<Vec<_>>(),vec![0,1]);std::fs::remove_file(path).unwrap();}
 #[test] fn python_roundtrip_when_requested(){if std::env::var_os("ATLAS_FAISS_CROSS_TEST").is_none(){return} let index=Index::load(Path::new("/private/tmp/atlas-faiss-python.index")).unwrap();let mut q=[0.;512];q[0]=1.;assert_eq!(index.search(&q,&[0,1],1).unwrap()[0].0,0);index.save(Path::new("/private/tmp/atlas-faiss-rust.index")).unwrap();}
}
