use std::{io,os::windows::io::{AsRawSocket,FromRawSocket},path::Path,sync::OnceLock,time::Instant};
use windows_sys::Win32::Networking::WinSock::*;
fn socket_error()->io::Error{io::Error::from_raw_os_error(unsafe{WSAGetLastError()})}
pub fn connect(path:&Path,deadline:Instant,is_cancelled:&dyn Fn()->bool)->io::Result<uds_windows::UnixStream>{
 static INIT:OnceLock<i32>=OnceLock::new();
 let status=*INIT.get_or_init(||unsafe{let mut data:WSADATA=std::mem::zeroed();WSAStartup(0x202,&mut data)});
 if status!=0{return Err(io::Error::from_raw_os_error(status))}
 let bytes=path.to_str().ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,"invalid socket path"))?.as_bytes();
 if bytes.is_empty()||bytes.len()>=108||bytes.contains(&0){return Err(io::Error::new(io::ErrorKind::InvalidInput,"socket path too long or invalid"))}
 let raw=unsafe{WSASocketW(AF_UNIX as i32,SOCK_STREAM,0,std::ptr::null(),0,WSA_FLAG_OVERLAPPED|WSA_FLAG_NO_HANDLE_INHERIT)};
 if raw==INVALID_SOCKET{return Err(socket_error())}
 let stream=unsafe{uds_windows::UnixStream::from_raw_socket(raw as _)};
 stream.set_nonblocking(true)?;
 let mut addr=SOCKADDR_UN::default();addr.sun_family=AF_UNIX;
 for (d,s) in addr.sun_path.iter_mut().zip(bytes){*d=*s as i8;}
 let result=unsafe{windows_sys::Win32::Networking::WinSock::connect(raw,(&addr as *const SOCKADDR_UN).cast(),(2+bytes.len()+1) as i32)};
 if result==SOCKET_ERROR {
  let err=socket_error();if !matches!(err.raw_os_error(),Some(10035)|Some(10036)){return Err(err)}
  loop {let left=deadline.checked_duration_since(Instant::now()).filter(|v|!v.is_zero()).ok_or_else(||io::Error::new(io::ErrorKind::TimedOut,"search deadline exceeded"))?;
   if is_cancelled(){return Err(io::Error::new(io::ErrorKind::Interrupted,"cancelled"))}
   let ms=left.as_millis().saturating_add(1).min(100) as i32;
   let mut item=WSAPOLLFD{fd:raw,events:POLLOUT,revents:0};let n=unsafe{WSAPoll(&mut item,1,ms)};
   if n==0{continue}
   if n==SOCKET_ERROR {let err=socket_error();if err.kind()==io::ErrorKind::Interrupted{continue}return Err(err)}
   let mut result=0i32;let mut length=std::mem::size_of::<i32>() as i32;
   if unsafe{getsockopt(raw,SOL_SOCKET,SO_ERROR,(&mut result as *mut i32).cast(),&mut length)}==SOCKET_ERROR{return Err(socket_error())}
   if result!=0{return Err(io::Error::from_raw_os_error(result))}break
  }
 }
 if Instant::now()>=deadline{return Err(io::Error::new(io::ErrorKind::TimedOut,"search deadline exceeded"))}
 Ok(stream)
}
pub fn ready(stream:&uds_windows::UnixStream,write:bool,deadline:Instant,is_cancelled:&dyn Fn()->bool)->io::Result<()>{
 loop{
  if is_cancelled(){return Err(io::Error::new(io::ErrorKind::Interrupted,"cancelled"))}
  let left=deadline.checked_duration_since(Instant::now()).filter(|v|!v.is_zero()).ok_or_else(||io::Error::new(io::ErrorKind::TimedOut,"search deadline exceeded"))?;
  let mut item=WSAPOLLFD{fd:stream.as_raw_socket() as _,events:if write{POLLOUT}else{POLLIN},revents:0};
  let n=unsafe{WSAPoll(&mut item,1,left.as_millis().saturating_add(1).min(100) as i32)};
  if n>0{return Ok(())}if n==0{continue}let e=socket_error();if e.kind()!=io::ErrorKind::Interrupted{return Err(e)}
 }
}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn expired_deadline_does_not_connect() {
  let key=super::super::windows_key::KeyFile::new().unwrap();let addr=std::path::PathBuf::from(super::super::windows_key::cli_path(&key.dir).unwrap()).join("model.sock");let _listener=uds_windows::UnixListener::bind(&addr).unwrap();
  let result=connect(&addr,Instant::now(),&||false);assert!(matches!(result,Err(ref e) if e.kind()==io::ErrorKind::TimedOut));
 }

}
