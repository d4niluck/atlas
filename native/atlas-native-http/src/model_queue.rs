//! Bounded FIFO admission for a single model. Separate models need separate queues.
use std::{collections::VecDeque, sync::{Condvar, Mutex}, time::{Duration, Instant}};

#[derive(Default)]
struct State { next: u64, active: bool, waiting: VecDeque<u64> }

pub struct ModelQueue { state: Mutex<State>, changed: Condvar, limit: usize }
pub struct Permit<'a> { queue: &'a ModelQueue }

impl ModelQueue {
    pub fn new(limit: usize) -> Self { Self { state: Mutex::new(State::default()), changed: Condvar::new(), limit } }

    /// Background indexing waits for space instead of failing a partially built index.
    pub fn acquire_background(&self, deadline: Instant, cancelled: impl Fn() -> bool) -> Result<Permit<'_>, &'static str> {
        loop {
            if cancelled() { return Err("cancelled"); }
            if Instant::now() >= deadline { return Err("timeout"); }
            match self.acquire(deadline, &cancelled) {
                Err("queue full") => std::thread::sleep(Duration::from_millis(25)),
                result => return result,
            }
        }
    }

    /// Cancellation and deadline are checked while waiting, and immediately before admission.
    pub fn acquire(&self, deadline: Instant, cancelled: impl Fn() -> bool) -> Result<Permit<'_>, &'static str> {
        let mut s = self.state.lock().map_err(|_| "queue unavailable")?;
        if s.waiting.len() >= self.limit && (s.active || !s.waiting.is_empty()) { return Err("queue full"); }
        let id = s.next; s.next = s.next.wrapping_add(1); s.waiting.push_back(id);
        loop {
            if cancelled() || Instant::now() >= deadline {
                s.waiting.retain(|ticket| *ticket != id); self.changed.notify_all();
                return Err(if cancelled() { "cancelled" } else { "timeout" });
            }
            if !s.active && s.waiting.front() == Some(&id) {
                s.waiting.pop_front(); s.active = true; return Ok(Permit { queue: self });
            }
            let wait = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(25));
            s = self.changed.wait_timeout(s, wait).map_err(|_| "queue unavailable")?.0;
        }
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) { if let Ok(mut s) = self.queue.state.lock() { s.active = false; self.queue.changed.notify_all(); } }
}

#[cfg(test)] mod tests {
    use super::*;
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
    #[test] fn fifo_cancel_and_full() {
        let q = Arc::new(ModelQueue::new(2)); let d=Instant::now()+Duration::from_secs(5);
        let held=q.acquire(d,||false).unwrap(); let (tx,rx)=mpsc::channel(); let stop=Arc::new(AtomicBool::new(false));
        let mut workers=vec![];
        for i in 0..2 { let worker_q=q.clone(); let tx=tx.clone(); let stop=stop.clone(); workers.push(std::thread::spawn(move || {let r=worker_q.acquire(d,||i==0 && stop.load(Ordering::SeqCst)); tx.send((i,r.is_ok())).unwrap();}));
            while q.state.lock().unwrap().waiting.len() != i+1 { std::thread::yield_now(); }
        }
        assert_eq!(q.acquire(d,||false).err(),Some("queue full")); stop.store(true,Ordering::SeqCst);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(),(0,false)); drop(held);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(),(1,true)); for w in workers { w.join().unwrap(); }
    }
    #[test] fn successful_requests_run_in_arrival_order() {
        let q=Arc::new(ModelQueue::new(3)); let d=Instant::now()+Duration::from_secs(5);
        let held=q.acquire(d,||false).unwrap(); let (tx,rx)=mpsc::channel(); let mut workers=vec![];
        for i in 0..3 { let worker_q=q.clone(); let tx=tx.clone(); workers.push(std::thread::spawn(move || { let _permit=worker_q.acquire(d,||false).unwrap(); tx.send(i).unwrap(); }));
            while q.state.lock().unwrap().waiting.len() != i+1 { std::thread::yield_now(); }
        }
        drop(held); assert_eq!((0..3).map(|_|rx.recv_timeout(Duration::from_secs(2)).unwrap()).collect::<Vec<_>>(),vec![0,1,2]);
        for w in workers { w.join().unwrap(); }
    }
    #[test] fn background_waits_for_space_and_can_be_cancelled() {
        let q=Arc::new(ModelQueue::new(1));let held=q.acquire(Instant::now()+Duration::from_secs(3),||false).unwrap();
        let (tx,rx)=mpsc::channel();let q2=q.clone();let first=std::thread::spawn(move||{let _p=q2.acquire(Instant::now()+Duration::from_secs(3),||false).unwrap();tx.send(1).unwrap();});
        while q.state.lock().unwrap().waiting.len()!=1 {std::thread::yield_now();}
        let stop=Arc::new(AtomicBool::new(false));let s=stop.clone();let q2=q.clone();let bg=std::thread::spawn(move||q2.acquire_background(Instant::now()+Duration::from_secs(3),||s.load(Ordering::SeqCst)).err());
        stop.store(true,Ordering::SeqCst);assert_eq!(bg.join().unwrap(),Some("cancelled"));drop(held);assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(),1);first.join().unwrap();
    }
    #[test] fn background_retries_after_full() {
        let q=Arc::new(ModelQueue::new(1));let held=q.acquire(Instant::now()+Duration::from_secs(3),||false).unwrap();
        let q2=q.clone();let foreground=std::thread::spawn(move||{let _p=q2.acquire(Instant::now()+Duration::from_secs(3),||false).unwrap();});
        while q.state.lock().unwrap().waiting.len()!=1 {std::thread::yield_now();}
        let q2=q.clone();let bg=std::thread::spawn(move||q2.acquire_background(Instant::now()+Duration::from_secs(3),||false).is_ok());
        drop(held);foreground.join().unwrap();assert!(bg.join().unwrap());
    }
    #[test] fn timeout_releases_ticket() {
        let q=ModelQueue::new(1); let held=q.acquire(Instant::now()+Duration::from_secs(1),||false).unwrap();
        assert_eq!(q.acquire(Instant::now()+Duration::from_millis(5),||false).err(),Some("timeout")); drop(held);
        assert!(q.acquire(Instant::now()+Duration::from_secs(1),||false).is_ok());
    }
}
