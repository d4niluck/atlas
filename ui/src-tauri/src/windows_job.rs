#![cfg(windows)]
use std::{ffi::c_void, io, os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle}, time::{Duration,Instant}};
#[repr(C)] #[derive(Default)] struct BasicLimit{process_time:i64,job_time:i64,flags:u32,min_working:usize,max_working:usize,process_count:u32,affinity:usize,priority:u32,scheduling:u32}
#[repr(C)] #[derive(Default)] struct IoCounters{read_ops:u64,write_ops:u64,other_ops:u64,read_bytes:u64,write_bytes:u64,other_bytes:u64}
#[repr(C)] #[derive(Default)] struct ExtendedLimit{basic:BasicLimit,io:IoCounters,process_memory:usize,job_memory:usize,peak_process:usize,peak_job:usize}
#[repr(C)] #[derive(Default)] struct Accounting {user_time:i64,kernel_time:i64,period_user:i64,period_kernel:i64,page_faults:u32,total_processes:u32,active_processes:u32,terminated_processes:u32}
#[link(name="kernel32")] unsafe extern "system" {
 fn CreateJobObjectW(attrs:*const c_void,name:*const u16)->*mut c_void;
 fn SetInformationJobObject(job:*mut c_void,class:u32,info:*const c_void,len:u32)->i32;
 fn QueryInformationJobObject(job:*mut c_void,class:u32,info:*mut c_void,len:u32,returned:*mut u32)->i32;
 fn TerminateJobObject(job:*mut c_void,exit_code:u32)->i32;
}
pub struct Job(OwnedHandle);
impl Job {
 pub fn create()->io::Result<Self>{let h=unsafe{CreateJobObjectW(std::ptr::null(),std::ptr::null())};if h.is_null(){return Err(io::Error::last_os_error())}let job=Self(unsafe{OwnedHandle::from_raw_handle(h)});let mut limits=ExtendedLimit::default();limits.basic.flags=0x2000;let ok=unsafe{SetInformationJobObject(job.0.as_raw_handle(),9,(&limits as *const ExtendedLimit).cast(),std::mem::size_of::<ExtendedLimit>() as u32)};if ok==0 {return Err(io::Error::last_os_error())}Ok(job)}
 pub fn stop(&self,timeout:Duration)->io::Result<()>{if unsafe{TerminateJobObject(self.0.as_raw_handle(),1)}==0{return Err(io::Error::last_os_error())}let start=Instant::now();loop{let mut info=Accounting::default();if unsafe{QueryInformationJobObject(self.0.as_raw_handle(),1,(&mut info as *mut Accounting).cast(),std::mem::size_of::<Accounting>() as u32,std::ptr::null_mut())}==0{return Err(io::Error::last_os_error())}if info.active_processes==0{return Ok(())}if start.elapsed()>=timeout{return Err(io::Error::new(io::ErrorKind::TimedOut,"Windows job did not terminate"))}std::thread::sleep(Duration::from_millis(20));}}
}
#[repr(C)] #[derive(Default)] struct StartupInfo { cb:u32,reserved:*mut u16,desktop:*mut u16,title:*mut u16,x:u32,y:u32,xsize:u32,ysize:u32,xcount:u32,ycount:u32,fill:u32,flags:u32,show:u16,reserved2len:u16,reserved2:*mut u8,stdin:*mut c_void,stdout:*mut c_void,stderr:*mut c_void }
#[repr(C)] #[derive(Default)] struct StartupInfoEx { startup:StartupInfo,attributes:*mut c_void }
#[repr(C)] #[derive(Default)] struct ProcessInfo { process:*mut c_void,thread:*mut c_void,pid:u32,tid:u32 }
#[link(name="kernel32")] unsafe extern "system" {
 fn InitializeProcThreadAttributeList(list:*mut c_void,count:u32,flags:u32,size:*mut usize)->i32;
 fn UpdateProcThreadAttribute(list:*mut c_void,flags:u32,attribute:usize,value:*const c_void,size:usize,previous:*mut c_void,returned:*mut usize)->i32;
 fn DeleteProcThreadAttributeList(list:*mut c_void);
 fn CreateProcessW(application:*const u16,command:*mut u16,process_attributes:*mut c_void,thread_attributes:*mut c_void,inherit:i32,flags:u32,environment:*mut c_void,directory:*const u16,startup:*const StartupInfo,info:*mut ProcessInfo)->i32;
 fn WaitForSingleObject(handle:*mut c_void,millis:u32)->u32;
 fn GetExitCodeProcess(handle:*mut c_void,code:*mut u32)->i32;
 fn GetProcessId(handle:*mut c_void)->u32;
}
struct Attributes(*mut c_void);
impl Drop for Attributes {fn drop(&mut self){unsafe{DeleteProcThreadAttributeList(self.0)}}}
#[repr(C)] struct SecurityAttrs {length:u32, descriptor:*mut c_void, inherit:i32}
#[link(name="kernel32")] unsafe extern "system" {
 fn CreatePipe(read:*mut *mut c_void,write:*mut *mut c_void,attrs:*mut SecurityAttrs,size:u32)->i32;
 fn SetHandleInformation(handle:*mut c_void,mask:u32,flags:u32)->i32;
 fn CreateFileW(name:*const u16,access:u32,share:u32,attrs:*mut SecurityAttrs,disposition:u32,flags:u32,template:*mut c_void)->*mut c_void;
}
pub struct ProcessPipe { process:OwnedHandle, pub input:Option<std::fs::File>, pub output:Option<std::fs::File>, ack:Option<std::sync::mpsc::Receiver<(io::Result<String>,std::fs::File)>>, startup:Option<std::sync::mpsc::Receiver<(io::Result<String>,std::fs::File)>>, acknowledged:bool }
fn quoted_arg(s:&str)->String {
 let mut result=String::from("\"");let mut slashes=0usize;
 for c in s.chars(){if c=='\\'{slashes+=1;continue}if c=='\"'{result.push_str(&"\\".repeat(slashes*2+1));result.push('"');slashes=0;continue}result.push_str(&"\\".repeat(slashes));slashes=0;result.push(c)}
 result.push_str(&"\\".repeat(slashes*2));result.push('"');result
}
fn make_pipe()->io::Result<(OwnedHandle,OwnedHandle)>{let mut r=std::ptr::null_mut();let mut w=std::ptr::null_mut();let mut sa=SecurityAttrs{length:std::mem::size_of::<SecurityAttrs>() as u32,descriptor:std::ptr::null_mut(),inherit:1};if unsafe{CreatePipe(&mut r,&mut w,&mut sa,0)}==0{return Err(io::Error::last_os_error())}Ok((unsafe{OwnedHandle::from_raw_handle(r)},unsafe{OwnedHandle::from_raw_handle(w)}))}
impl Job {
 pub fn spawn(&self,exe:&str,args:&[String])->io::Result<ProcessPipe>{use std::os::windows::io::IntoRawHandle;
  let (in_r,in_w)=make_pipe()?;let (out_r,out_w)=make_pipe()?;
  for h in [&in_w,&out_r]{if unsafe{SetHandleInformation(h.as_raw_handle(),1,0)}==0{return Err(io::Error::last_os_error())}}
  let nul:Vec<u16>="NUL\0".encode_utf16().collect();let mut sa=SecurityAttrs{length:std::mem::size_of::<SecurityAttrs>() as u32,descriptor:std::ptr::null_mut(),inherit:1};
  let raw=unsafe{CreateFileW(nul.as_ptr(),0x40000000,3,&mut sa,3,0,std::ptr::null_mut())};if raw as isize == -1{return Err(io::Error::last_os_error())}let err=unsafe{OwnedHandle::from_raw_handle(raw)};
  let inherited=[in_r.as_raw_handle(),out_w.as_raw_handle(),err.as_raw_handle()];let job=self.0.as_raw_handle();
  let mut needed=0usize;unsafe{InitializeProcThreadAttributeList(std::ptr::null_mut(),2,0,&mut needed)};if needed==0{return Err(io::Error::last_os_error())}
  let mut mem=vec![0usize;needed.div_ceil(std::mem::size_of::<usize>())];let ptr=mem.as_mut_ptr().cast();if unsafe{InitializeProcThreadAttributeList(ptr,2,0,&mut needed)}==0{return Err(io::Error::last_os_error())}let _attrs=Attributes(ptr);
  if unsafe{UpdateProcThreadAttribute(ptr,0,0x0002_000d,(&job as *const *mut c_void).cast(),std::mem::size_of_val(&job),std::ptr::null_mut(),std::ptr::null_mut())}==0{return Err(io::Error::last_os_error())}
  if unsafe{UpdateProcThreadAttribute(ptr,0,0x0002_0002,inherited.as_ptr().cast(),std::mem::size_of_val(&inherited),std::ptr::null_mut(),std::ptr::null_mut())}==0{return Err(io::Error::last_os_error())}
  let mut start=StartupInfoEx::default();start.startup.cb=std::mem::size_of::<StartupInfoEx>() as u32;start.startup.flags=0x100;start.startup.stdin=in_r.as_raw_handle();start.startup.stdout=out_w.as_raw_handle();start.startup.stderr=err.as_raw_handle();start.attributes=ptr;
  if exe.contains('\0')||args.iter().any(|a|a.contains('\0')){return Err(io::Error::new(io::ErrorKind::InvalidInput,"NUL in executable or arguments"))};let app:Vec<u16>=exe.encode_utf16().chain(std::iter::once(0)).collect();let mut line=quoted_arg(exe);for arg in args{line.push(' ');line.push_str(&quoted_arg(arg))}let mut cmd:Vec<u16>=line.encode_utf16().chain(std::iter::once(0)).collect();let mut info=ProcessInfo::default();
  if unsafe{CreateProcessW(app.as_ptr(),cmd.as_mut_ptr(),std::ptr::null_mut(),std::ptr::null_mut(),1,0x0008_0000|0x0800_0000,std::ptr::null_mut(),std::ptr::null(),&start.startup,&mut info)}==0{return Err(io::Error::last_os_error())}
  let process=unsafe{OwnedHandle::from_raw_handle(info.process)};let _thread=unsafe{OwnedHandle::from_raw_handle(info.thread)};
  Ok(ProcessPipe{process,input:Some(unsafe{std::fs::File::from_raw_handle(in_w.into_raw_handle())}),output:Some(unsafe{std::fs::File::from_raw_handle(out_r.into_raw_handle())}),ack:None,startup:None,acknowledged:false})
 }
}

impl ProcessPipe {
 pub fn id(&self)->u32 {unsafe{GetProcessId(self.process.as_raw_handle())}}
 pub fn startup(&mut self,input:&str,timeout:Duration)->io::Result<u16> {
  use std::io::{Read,Write};
  if self.startup.is_none(){self.input.as_mut().ok_or_else(||io::Error::other("missing stdin"))?.write_all(input.as_bytes())?;let mut stdout=self.output.take().ok_or_else(||io::Error::other("missing stdout"))?;let (tx,rx)=std::sync::mpsc::sync_channel(1);
   std::thread::spawn(move||{let result=(||->io::Result<String>{let mut bytes=Vec::new();while bytes.len()<256{let mut b=[0];if stdout.read(&mut b)?==0{return Err(io::Error::other("startup reply missing"))}bytes.push(b[0]);if b[0]==b'\n'{return String::from_utf8(bytes).map_err(io::Error::other)}}Err(io::Error::other("invalid startup reply"))})();let _=tx.send((result,stdout));});self.startup=Some(rx);}
  let (line,stdout)=self.startup.as_ref().unwrap().recv_timeout(timeout).map_err(|e|match e{std::sync::mpsc::RecvTimeoutError::Timeout=>io::Error::new(io::ErrorKind::TimedOut,e),std::sync::mpsc::RecvTimeoutError::Disconnected=>io::Error::new(io::ErrorKind::BrokenPipe,e)})?;self.startup=None;self.output=Some(stdout);line?.trim().parse().map_err(io::Error::other)
 }
 pub fn has_exited(&self)->io::Result<bool>{match unsafe{WaitForSingleObject(self.process.as_raw_handle(),0)}{0=>Ok(true),258=>Ok(false),_=>Err(io::Error::last_os_error())}}
 pub fn shutdown(&mut self,timeout:Duration)->io::Result<()> {
  use std::io::{Read,Write};let started=Instant::now();
  if !self.acknowledged && self.ack.is_none(){
   self.input.as_mut().ok_or_else(||io::Error::other("control input unavailable"))?.write_all(b"shutdown\n")?;
   let mut output=self.output.take().ok_or_else(||io::Error::other("control output unavailable"))?;
   let (tx,rx)=std::sync::mpsc::sync_channel(1);std::thread::spawn(move||{let mut bytes=Vec::new();let result=(||->io::Result<String>{while bytes.len()<128 {let mut b=[0];if output.read(&mut b)?==0{return Err(io::Error::other("shutdown reply missing"))}bytes.push(b[0]);if b[0]==b'\n'{return String::from_utf8(bytes).map_err(io::Error::other)}}Err(io::Error::other("invalid shutdown reply"))})();let _=tx.send((result,output));});self.ack=Some(rx);
  }
  if !self.acknowledged {let wait=timeout.saturating_sub(started.elapsed());let (ack,output)=self.ack.as_ref().unwrap().recv_timeout(wait).map_err(|e|match e{std::sync::mpsc::RecvTimeoutError::Timeout=>io::Error::new(io::ErrorKind::TimedOut,e),std::sync::mpsc::RecvTimeoutError::Disconnected=>io::Error::new(io::ErrorKind::BrokenPipe,e)})?;self.ack=None;self.output=Some(output);if ack?.trim()!="OK"{return Err(io::Error::other("shutdown not acknowledged"))}self.acknowledged=true;}
  let millis=timeout.saturating_sub(started.elapsed()).as_millis().min(u32::MAX as u128) as u32;
  match unsafe{WaitForSingleObject(self.process.as_raw_handle(),millis)} {0=>{let mut code=0;if unsafe{GetExitCodeProcess(self.process.as_raw_handle(),&mut code)}==0{return Err(io::Error::last_os_error())}if code!=0{return Err(io::Error::other(format!("service exited with {code}")))}Ok(())},258=>Err(io::Error::new(io::ErrorKind::TimedOut,"service did not stop")),_=>Err(io::Error::last_os_error())}
 }
}
pub fn shutdown_services(children:&mut [ProcessPipe],timeout:Duration)->io::Result<()> {
 if children.is_empty(){return Ok(())}if children.len()!=2{return Err(io::Error::other("unexpected service count"))}
 let (backend,proxy)=children.split_at_mut(1);if !proxy[0].has_exited()?{proxy[0].shutdown(timeout)?;}backend[0].shutdown(timeout)
}
#[cfg(test)] mod integration_tests {use super::*;use std::io::{BufRead,BufReader,Write};
 #[test]fn helper(){if std::env::var_os("ATLAS_JOB_PIPE_HELPER").is_none(){return}let mut s=String::new();std::io::stdin().read_line(&mut s).unwrap();assert_eq!(s,"test secret\n");println!("54321");std::io::stdout().flush().unwrap();std::thread::sleep(Duration::from_secs(10));}
 #[test]fn exchanges_secret_and_stops(){unsafe{std::env::set_var("ATLAS_JOB_PIPE_HELPER","1")};let job=Job::create().unwrap();let exe=std::env::current_exe().unwrap();let mut proc=job.spawn(exe.to_str().unwrap(),&["--exact".into(),"integration_tests::helper".into(),"--nocapture".into()]).unwrap();unsafe{std::env::remove_var("ATLAS_JOB_PIPE_HELPER")};proc.input.take().unwrap().write_all(b"test secret\n").unwrap();let mut r=BufReader::new(proc.output.take().unwrap());let mut line=String::new();loop{line.clear();assert!(r.read_line(&mut line).unwrap()>0);if line.trim()=="54321"{break}}job.stop(Duration::from_secs(5)).unwrap();assert_eq!(unsafe{WaitForSingleObject(proc.process.as_raw_handle(),5000)},0);}
 #[test] fn quote_examples(){assert_eq!(quoted_arg(""),"\"\"");assert_eq!(quoted_arg("a b"),"\"a b\"");assert_eq!(quoted_arg("a\\\"b"),"\"a\\\\\\\"b\"");assert_eq!(quoted_arg("c:\\путь\\"),"\"c:\\путь\\\\\"");}
}

#[cfg(test)]mod quoting_roundtrip {use super::*;
 #[link(name="shell32")]unsafe extern "system"{fn CommandLineToArgvW(cmd:*const u16,argc:*mut i32)->*mut *mut u16;}
 #[link(name="kernel32")]unsafe extern "system"{fn LocalFree(ptr:*mut c_void)->*mut c_void;}
 #[test]fn roundtrips_windows_paths_and_quotes(){for source in ["", "ordinary", "with spaces", "Привет 😀", r"C:\\каталог с пробелом\\файл\\", "quoted \\\" phrase", "tabs\tand\nlines"]{let command=format!("exe {}",quoted_arg(source));let wide:Vec<u16>=command.encode_utf16().chain(std::iter::once(0)).collect();let mut n=0;let ptr=unsafe{CommandLineToArgvW(wide.as_ptr(),&mut n)};assert!(!ptr.is_null());assert_eq!(n,2);let value=unsafe{*ptr.add(1)};let len=unsafe{(0..).find(|i|*value.add(*i)==0).unwrap()};let value=String::from_utf16(unsafe{std::slice::from_raw_parts(value,len)}).unwrap();unsafe{LocalFree(ptr.cast());};assert_eq!(value,source);}}
}
#[cfg(test)]mod shutdown_tests {use super::*;use std::{io::{Read},path::PathBuf};
 fn start(mode:&str)->(Job,ProcessPipe){let mut path=std::env::current_exe().unwrap();path.set_file_name("control-child.exe");let path:PathBuf=path;let job=Job::create().unwrap();let mut child=job.spawn(path.to_str().unwrap(),&[mode.into()]).unwrap();let mut startup=Vec::new();loop{let mut b=[0];child.output.as_mut().unwrap().read_exact(&mut b).unwrap();if b[0]==b'\n'{break}startup.push(b[0]);}assert_eq!(startup,b"12345");(job,child)}
 #[test]fn startup_and_shutdown_share_control_pipe(){let mut path=std::env::current_exe().unwrap();path.set_file_name("control-child.exe");let _job=Job::create().unwrap();let mut child=_job.spawn(path.to_str().unwrap(),&["startup".into()]).unwrap();assert_eq!(child.startup("token\n",Duration::from_secs(5)).unwrap(),12345);child.shutdown(Duration::from_secs(5)).unwrap();}
 #[test]fn startup_timeout_does_not_kill_and_retry_uses_same_reader(){let mut path=std::env::current_exe().unwrap();path.set_file_name("control-child.exe");let _job=Job::create().unwrap();let mut child=_job.spawn(path.to_str().unwrap(),&["late".into()]).unwrap();assert_eq!(child.startup("token\n",Duration::from_millis(100)).unwrap_err().kind(),io::ErrorKind::TimedOut);assert!(!child.has_exited().unwrap());assert_eq!(child.startup("MUST NOT BE SENT\n",Duration::from_secs(5)).unwrap(),12345);child.shutdown(Duration::from_secs(5)).unwrap();}
 #[test]fn ack_with_nonzero_exit_is_failure(){let (_job,mut child)=start("badexit");assert!(child.shutdown(Duration::from_secs(5)).is_err());}
 #[test]fn eof_without_ack_is_failure(){let (_job,mut child)=start("noack");assert!(child.shutdown(Duration::from_secs(5)).is_err());}
 fn start_in(job:&Job,mode:&str)->ProcessPipe{let mut path=std::env::current_exe().unwrap();path.set_file_name("control-child.exe");let mut child=job.spawn(path.to_str().unwrap(),&[mode.into()]).unwrap();let mut bytes=Vec::new();loop{let mut b=[0];child.output.as_mut().unwrap().read_exact(&mut b).unwrap();if b[0]==b'\n'{break}bytes.push(b[0]);}assert_eq!(bytes,b"12345");child}
 #[test]fn drains_backend_even_when_proxy_exited(){let job=Job::create().unwrap();let backend=start_in(&job,"ok");let proxy=start_in(&job,"gone");assert_eq!(unsafe{WaitForSingleObject(proxy.process.as_raw_handle(),5000)},0);let mut children=[backend,proxy];shutdown_services(&mut children,Duration::from_secs(5)).unwrap();}
 #[test]fn failed_backend_drain_keeps_job_alive_and_can_retry(){let job=Job::create().unwrap();let backend=start_in(&job,"slow");let proxy=start_in(&job,"ok");let mut children=[backend,proxy];assert_eq!(shutdown_services(&mut children,Duration::from_millis(100)).unwrap_err().kind(),io::ErrorKind::TimedOut);assert!(!children[0].has_exited().unwrap());shutdown_services(&mut children,Duration::from_secs(5)).unwrap();}
 #[test]fn waits_for_ack_and_exit_zero(){let (_job,mut child)=start("ok");child.shutdown(Duration::from_secs(5)).unwrap();}
 #[test]fn timeout_does_not_kill_and_later_retry_succeeds(){let (_job,mut child)=start("slow");assert_eq!(child.shutdown(Duration::from_millis(100)).unwrap_err().kind(),io::ErrorKind::TimedOut);assert_eq!(unsafe{WaitForSingleObject(child.process.as_raw_handle(),0)},258);child.shutdown(Duration::from_secs(5)).unwrap();}
 #[test]fn bad_reply_can_be_retried_without_losing_control_pipe(){let (_job,mut child)=start("retry");assert!(child.shutdown(Duration::from_secs(2)).is_err());assert!(!child.has_exited().unwrap());child.shutdown(Duration::from_secs(5)).unwrap();}
 #[test]fn bad_reply_does_not_kill_process(){let (_job,mut child)=start("bad");assert!(child.shutdown(Duration::from_secs(2)).is_err());assert_eq!(unsafe{WaitForSingleObject(child.process.as_raw_handle(),0)},258);}
}
#[cfg(test)]mod sequence_tests {use super::*;use std::io::Write;
 #[test]#[ignore]fn real_proxy_then_fake_backend_then_graceful_shutdown(){let proxy=std::env::var_os("ATLAS_TEST_PROXY").expect("ATLAS_TEST_PROXY is required");let ui=std::env::var("ATLAS_TEST_UI").unwrap();let mut backend=std::env::current_exe().unwrap();backend.set_file_name("control-child.exe");let job=Job::create().unwrap();
 let mut proxy=job.spawn(proxy.to_str().unwrap(),&[ui,"0".into(),"0".into(),"--backend-stdin".into()]).unwrap();let port=proxy.startup("bootstrap-only-1234567890abcdefgh\nbackend-token-1234567890abcdefghij\n",Duration::from_secs(5)).unwrap();assert!(port>0);
 let mut backend=job.spawn(backend.to_str().unwrap(),&["startup".into()]).unwrap();let port=backend.startup("token\n",Duration::from_secs(5)).unwrap();proxy.input.as_mut().unwrap().write_all(format!("{port}\n").as_bytes()).unwrap();let mut children=[backend,proxy];shutdown_services(&mut children,Duration::from_secs(5)).unwrap();}
}
