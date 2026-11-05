use std::{ops::Deref, sync::{Mutex,MutexGuard}, time::{Duration,Instant}};
use atlas_llama_probe::Generator;

struct Slot { enabled:bool, generator:Option<Generator>, exe:String, path:String, device:String, last_used:Instant }
pub struct RagModel(Mutex<Slot>);
pub struct RagGuard<'a>(MutexGuard<'a,Slot>);
impl Deref for RagGuard<'_> {type Target=Generator;fn deref(&self)->&Generator {self.0.generator.as_ref().expect("loaded generator")}}
impl Drop for RagGuard<'_> {fn drop(&mut self){self.0.last_used=Instant::now();}}
impl RagModel {
 pub fn new(generator:Generator,exe:String,path:String,device:String)->Self {Self(Mutex::new(Slot{enabled:true,generator:Some(generator),exe,path,device,last_used:Instant::now()}))}
 pub fn lazy(exe:String,path:String,device:String)->Self {Self(Mutex::new(Slot{enabled:true,generator:None,exe,path,device,last_used:Instant::now()}))}
 pub fn acquire(&self)->Result<RagGuard<'_>,String> {let mut slot=loop {match self.0.try_lock(){Ok(g)=>break g,Err(std::sync::TryLockError::WouldBlock)=>{atlas_llama_probe::remaining_search_time()?;std::thread::sleep(Duration::from_millis(25));},Err(_)=>return Err("model unavailable".into())}};if !slot.enabled{return Err("Умный поиск выключен".into())}if slot.generator.is_none(){slot.generator=Some(Generator::start_device(&slot.exe,&slot.path,&slot.device).map_err(|e|if slot.device=="metal"{format!("Не удалось запустить модель на GPU Apple. Выберите CPU в настройках. {e}")}else{e})?);}Ok(RagGuard(slot))}
 pub fn reconfigure(&self,path:&str,device:&str,enabled:bool)->Result<(),String>{let mut slot=self.0.lock().map_err(|_|"model unavailable")?;if slot.path!=path||slot.device!=device||!enabled {slot.generator.take();}slot.path=path.to_owned();slot.device=device.to_owned();slot.enabled=enabled;Ok(())}
 pub fn active_device(&self)->Result<Option<String>,()>{self.0.try_lock().map(|slot|slot.generator.as_ref().map(|_|slot.device.clone())).map_err(|_|())}
 pub fn unload_if_idle(&self,age:Duration)->bool {if let Ok(mut slot)=self.0.try_lock(){if slot.last_used.elapsed()>=age {slot.generator.take();return true}}false}
}

#[cfg(test)] mod tests {use super::*;#[test] fn changes_unloaded_configuration(){let m=RagModel::lazy("exe".into(),"old".into(),"cpu".into());m.reconfigure("new","metal",true).unwrap();let s=m.0.lock().unwrap();assert_eq!(s.path,"new");assert_eq!(s.device,"metal");assert!(s.generator.is_none());}}
