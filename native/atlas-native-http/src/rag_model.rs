use std::{ops::Deref, sync::{Mutex,MutexGuard}, time::{Duration,Instant}};
use atlas_llama_probe::Generator;

struct Slot { generator:Option<Generator>, exe:String, path:String, device:String, last_used:Instant }
pub struct RagModel(Mutex<Slot>);
pub struct RagGuard<'a>(MutexGuard<'a,Slot>);
impl Deref for RagGuard<'_> {type Target=Generator;fn deref(&self)->&Generator {self.0.generator.as_ref().expect("loaded generator")}}
impl Drop for RagGuard<'_> {fn drop(&mut self){self.0.last_used=Instant::now();}}
impl RagModel {
 pub fn new(generator:Generator,exe:String,path:String,device:String)->Self {Self(Mutex::new(Slot{generator:Some(generator),exe,path,device,last_used:Instant::now()}))}
 pub fn acquire(&self)->Result<RagGuard<'_>,String> {let mut slot=loop {match self.0.try_lock(){Ok(g)=>break g,Err(std::sync::TryLockError::WouldBlock)=>{atlas_llama_probe::remaining_search_time()?;std::thread::sleep(Duration::from_millis(25));},Err(_)=>return Err("model unavailable".into())}};if slot.generator.is_none(){slot.generator=Some(Generator::start_device(&slot.exe,&slot.path,&slot.device)?);}Ok(RagGuard(slot))}
 pub fn unload_if_idle(&self,age:Duration)->bool {if let Ok(mut slot)=self.0.try_lock(){if slot.last_used.elapsed()>=age {slot.generator.take();return true}}false}
}
