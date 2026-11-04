use std::sync::{Arc,Condvar,Mutex};
use std::time::Instant;

#[derive(Default)]
struct State { closing:bool, active:usize, failed:bool }
#[derive(Default)]
pub struct Gate { state:Mutex<State>, changed:Condvar }
pub struct Lease(Arc<Gate>);
impl Gate {
 pub fn enter(self:&Arc<Self>)->Option<Lease>{let mut s=self.state.lock().ok()?;if s.closing{return None}s.active=s.active.checked_add(1)?;drop(s);Some(Lease(self.clone()))}
 pub fn close(&self)->Result<(),()> {let mut s=self.state.lock().map_err(|_|())?;s.closing=true;self.changed.notify_all();Ok(())}
 pub fn close_when_idle(&self,deadline:Instant)->Result<(),()> {let mut s=self.state.lock().map_err(|_|())?;loop {if Instant::now()>=deadline{return Err(())}if s.failed{return Err(())}if s.active==0 {s.closing=true;self.changed.notify_all();return Ok(())}let wait=deadline.saturating_duration_since(Instant::now());let (next,_)=self.changed.wait_timeout(s,wait).map_err(|_|())?;s=next;}}
 pub fn is_closed(&self)->bool {self.state.lock().map_or(true,|s|s.closing)}
 pub fn wait_until(&self,deadline:Instant)->Result<(),()> {let mut s=self.state.lock().map_err(|_|())?;loop {if Instant::now()>=deadline{return Err(())}if s.failed{return Err(())}if s.active==0{return Ok(())}let wait=deadline.saturating_duration_since(Instant::now());let (next,_)=self.changed.wait_timeout(s,wait).map_err(|_|())?;s=next;}}
}
impl Drop for Lease {fn drop(&mut self){if let Ok(mut s)=self.0.state.lock(){s.active-=1;if std::thread::panicking(){s.failed=true;}self.0.changed.notify_all();}}}
static BACKGROUND:std::sync::OnceLock<Arc<Gate>>=std::sync::OnceLock::new();
pub fn background()->&'static Arc<Gate>{BACKGROUND.get_or_init(||Arc::new(Gate::default()))}
static THREADS:std::sync::OnceLock<Arc<Gate>>=std::sync::OnceLock::new();
pub fn threads()->&'static Arc<Gate>{THREADS.get_or_init(||Arc::new(Gate::default()))}
#[cfg(test)] mod tests {
 use super::*;use std::{thread,time::Duration};
 #[test]fn close_rejects_new_and_waits_for_active(){let gate=Arc::new(Gate::default());let permit=gate.enter().unwrap();gate.close().unwrap();assert!(gate.enter().is_none());assert!(gate.wait_until(Instant::now()+Duration::from_millis(50)).is_err());drop(permit);assert!(gate.wait_until(Instant::now()+Duration::from_secs(1)).is_ok());}
 #[test]fn panic_prevents_shutdown_acknowledgement(){let gate=Arc::new(Gate::default());let p=gate.enter().unwrap();let _=thread::spawn(move||{let _p=p;panic!("expected")}).join();gate.close().unwrap();assert!(gate.wait_until(Instant::now()+Duration::from_secs(1)).is_err());}
 #[test]fn competing_close_and_enter_never_accept_after_closing(){for _ in 0..200{let gate=Arc::new(Gate::default());let clone=gate.clone();let t=thread::spawn(move||{let _=clone.enter();});gate.close().unwrap();t.join().unwrap();assert!(gate.enter().is_none());assert!(gate.wait_until(Instant::now()+Duration::from_secs(1)).is_ok());}}
}
