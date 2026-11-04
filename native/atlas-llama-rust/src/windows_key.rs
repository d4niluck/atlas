use std::{ffi::{c_void, OsStr}, io::{self}, os::windows::ffi::OsStrExt, path::PathBuf, ptr};
type Handle=*mut c_void;
#[link(name="kernel32")] unsafe extern "system" {
 fn GetCurrentProcess()->Handle;
 fn CloseHandle(h:Handle)->i32;
 fn LocalFree(h:*mut c_void)->*mut c_void;
 fn GetShortPathNameW(long:*const u16,short:*mut u16,size:u32)->u32;
}
#[link(name="advapi32")] unsafe extern "system" {
 fn OpenProcessToken(process:Handle,access:u32,token:*mut Handle)->i32;
 fn GetTokenInformation(token:Handle,class:u32,info:*mut c_void,len:u32,needed:*mut u32)->i32;
 fn ConvertSidToStringSidW(sid:*mut c_void,string:*mut *mut u16)->i32;
 fn ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl:*const u16,revision:u32,out:*mut *mut c_void,size:*mut u32)->i32;
}
#[link(name="bcrypt")] unsafe extern "system" {fn BCryptGenRandom(algorithm:Handle,buffer:*mut u8,length:u32,flags:u32)->i32;}
struct Local(*mut c_void);
impl Drop for Local {fn drop(&mut self) {unsafe{LocalFree(self.0);}}}
struct Token(Handle);
impl Drop for Token {fn drop(&mut self) {unsafe{CloseHandle(self.0);}}}
fn current_user_sid()->io::Result<String> {
 let mut h=ptr::null_mut();if unsafe{OpenProcessToken(GetCurrentProcess(),8,&mut h)}==0{return Err(io::Error::last_os_error())};let token=Token(h);
 let mut len=0;unsafe{GetTokenInformation(token.0,1,ptr::null_mut(),0,&mut len)};if len==0{return Err(io::Error::last_os_error())}
 let mut data=vec![0usize;(len as usize).div_ceil(std::mem::size_of::<usize>())];
 if unsafe{GetTokenInformation(token.0,1,data.as_mut_ptr().cast(),len,&mut len)}==0{return Err(io::Error::last_os_error())}
 let sid=unsafe{*(data.as_ptr() as *const *mut c_void)};let mut ptr=ptr::null_mut();
 if unsafe{ConvertSidToStringSidW(sid,&mut ptr)}==0{return Err(io::Error::last_os_error())};let guard=Local(ptr.cast());
 let mut n=0usize;unsafe{while *ptr.add(n)!=0{n+=1}};let s=String::from_utf16(unsafe{std::slice::from_raw_parts(ptr,n)}).map_err(|_|io::Error::other("invalid SID"))?;drop(guard);Ok(s)
}
fn create_private_dir(parent:&std::path::Path,name:&OsStr)->io::Result<super::windows_temp::PrivateTemp> {
 let sid=current_user_sid()?;let sddl=format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)");
 let sddl:Vec<u16>=OsStr::new(&sddl).encode_wide().chain(Some(0)).collect();let mut sd=ptr::null_mut();
 if unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(),1,&mut sd,ptr::null_mut())}==0{return Err(io::Error::last_os_error())};let guard=Local(sd);
 let result=super::windows_temp::PrivateTemp::new(parent,name,sd);drop(guard);result
}
// llama.cpp b12001 accepts UTF-8 command line paths; only AF_UNIX socket addresses are size-limited.
pub fn cli_path(path:&std::path::Path)->Result<String,String>{path.to_str().map(str::to_owned).ok_or_else(||"invalid Windows path".to_owned())}
pub fn socket_dir_path(path:&std::path::Path)->Result<String,String>{
 let original=cli_path(path)?;if original.len()+"\\model.sock".len()<108{return Ok(original)}
 let wide:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
 let required=unsafe{GetShortPathNameW(wide.as_ptr(),ptr::null_mut(),0)};if required==0{return Err("socket path exceeds Windows AF_UNIX limit and short path is unavailable".into())}
 let mut short=vec![0u16;required as usize+1];let len=unsafe{GetShortPathNameW(wide.as_ptr(),short.as_mut_ptr(),short.len() as u32)};
 if len==0||len as usize>=short.len(){return Err("cannot resolve a short Windows socket path".into())}
 short.truncate(len as usize);let result=String::from_utf16(&short).map_err(|_|"invalid short Windows path")?;
 if result.len()+"\\model.sock".len()>=108{return Err("socket path exceeds Windows AF_UNIX limit".into())}Ok(result)
}
pub struct KeyFile { pub dir:PathBuf, pub token:String, _private:super::windows_temp::PrivateTemp }
impl KeyFile { pub fn new()->Result<Self,String> {
 let mut bytes=[0u8;32];let status=unsafe{BCryptGenRandom(ptr::null_mut(),bytes.as_mut_ptr(),32,2)};if status<0{return Err(format!("random generation failed: {status:#x}"))}
 let token=bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();let name=format!("atlas-{}-{}",std::process::id(),&token[..16]);
 let private=create_private_dir(&std::env::temp_dir(),OsStr::new(&name)).map_err(|e|e.to_string())?;let dir=private.path.clone();Ok(Self{dir,token,_private:private})
} }
#[cfg(test)] mod tests {
 use super::*;
 #[link(name="advapi32")] unsafe extern "system" {
  fn GetNamedSecurityInfoW(path:*const u16,object_type:u32,information:u32,owner:*mut *mut c_void,group:*mut *mut c_void,dacl:*mut *mut c_void,sacl:*mut *mut c_void,descriptor:*mut *mut c_void)->u32;
  fn ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor:*mut c_void,revision:u32,info:u32,result:*mut *mut u16,len:*mut u32)->i32;
 }
 fn sddl(path:&std::path::Path)->String {
  let wide:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();let mut sd=ptr::null_mut();
  assert_eq!(unsafe{GetNamedSecurityInfoW(wide.as_ptr(),1,4,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),&mut sd)},0);
  let _guard=Local(sd);let mut out=ptr::null_mut();let mut len=0;
  assert_ne!(unsafe{ConvertSecurityDescriptorToStringSecurityDescriptorW(sd,1,4,&mut out,&mut len)},0);
  let _guard=Local(out.cast());String::from_utf16(unsafe{std::slice::from_raw_parts(out,(len-1) as usize)}).unwrap()
 }
 #[test] fn socket_dir_prefers_unicode_and_rejects_unresolvable_long_path() {
  let short=std::path::Path::new(r"C:\атлас");assert_eq!(socket_dir_path(short).unwrap(),short.to_str().unwrap());
  let long=std::path::PathBuf::from(format!(r"C:\{}", "x".repeat(120)));assert!(socket_dir_path(&long).is_err());
 }
 #[test] fn runtime_dir_is_restricted_to_user_and_system() {
  let key=KeyFile::new().unwrap();let sid=current_user_sid().unwrap();
  let dir=sddl(&key.dir);
  for value in [&dir] {assert!(value.contains(&sid),"{value}");assert!(value.contains(";;;SY)"),"{value}");assert!(!value.contains(";;;WD)"),"{value}");assert!(!value.contains(";;;BU)"),"{value}");assert!(!value.contains(";;;AU)"),"{value}");}
  assert!(dir.starts_with("D:P"),"{dir}");
 }
}
