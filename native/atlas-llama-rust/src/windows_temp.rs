//! Private, descriptor-anchored runtime directory on local NTFS.
use std::{ffi::{c_void,OsStr},fs::File,io,mem::size_of,os::windows::{ffi::OsStrExt,io::{AsRawHandle,FromRawHandle}},path::{Component,Path,PathBuf,Prefix},ptr};
type Handle=*mut c_void;
const ACCESS:u32=0x80|0x20|0x0010_0000;
const SHARE_ANCESTOR:u32=3;
const SHARE_PRIVATE:u32=1;
const OPEN_REPARSE:u32=0x0020_0000;
#[repr(C)] struct AttrTag {attrs:u32,tag:u32}
#[repr(C)] struct UString {len:u16,max:u16,buf:*mut u16}
#[repr(C)] struct ObjAttr {len:u32,root:Handle,name:*mut UString,attrs:u32,security:Handle,qos:Handle}
#[repr(C)] struct IoStatus {status:usize,info:usize}
#[link(name="kernel32")] unsafe extern "system" {
 fn CreateFileW(name:*const u16,access:u32,share:u32,security:Handle,disposition:u32,flags:u32,template:Handle)->Handle;
 fn GetFileInformationByHandleEx(file:Handle,class:u32,info:*mut c_void,size:u32)->i32;
 fn GetDriveTypeW(root:*const u16)->u32;
 fn GetVolumeInformationByHandleW(file:Handle,name:*mut u16,name_len:u32,serial:*mut u32,max_component:*mut u32,flags:*mut u32,filesystem:*mut u16,filesystem_len:u32)->i32;
 fn SetFileInformationByHandle(file:Handle,class:u32,info:*const c_void,size:u32)->i32;
}
#[link(name="ntdll")] unsafe extern "system" {
 fn NtCreateFile(file:*mut Handle,access:u32,attr:*mut ObjAttr,io:*mut IoStatus,size:*mut i64,file_attr:u32,share:u32,disposition:u32,options:u32,ea:*mut c_void,ea_len:u32)->i32;
}
fn invalid(msg:&'static str)->io::Error {io::Error::new(io::ErrorKind::InvalidInput,msg)}
fn validate_dir(file:&File)->io::Result<()> {
 let mut info=AttrTag{attrs:0,tag:0};
 if unsafe{GetFileInformationByHandleEx(file.as_raw_handle(),9,(&mut info as *mut AttrTag).cast(),size_of::<AttrTag>() as u32)}==0{return Err(io::Error::last_os_error())}
 if info.attrs&0x400!=0 {return Err(io::Error::new(io::ErrorKind::PermissionDenied,"reparse point forbidden"))}
 if info.attrs&0x10==0 {return Err(invalid("directory required"))}Ok(())
}
fn open_dir(parent:&File,name:&OsStr,create:bool,descriptor:Handle,share:u32)->io::Result<File>{
 let mut name:Vec<u16>=name.encode_wide().collect();
 if name.is_empty()||name.len()>255||name.iter().any(|c|matches!(*c,0|47|58|92))||matches!(name.last(),Some(32|46)){return Err(invalid("invalid path component"))}
 let len:u16=(name.len()*2).try_into().map_err(|_|invalid("component too long"))?;
 let mut unicode=UString{len,max:len,buf:name.as_mut_ptr()};
 let mut attr=ObjAttr{len:size_of::<ObjAttr>() as u32,root:parent.as_raw_handle(),name:&mut unicode,attrs:0x40,security:descriptor,qos:ptr::null_mut()};
 let mut status=IoStatus{status:0,info:0};let mut handle:Handle=ptr::null_mut();
 let access=ACCESS|if create{0x0001_0000}else{0};
 let code=unsafe{NtCreateFile(&mut handle,access,&mut attr,&mut status,ptr::null_mut(),0x80,share,if create{2}else{1},0x20|1|OPEN_REPARSE,ptr::null_mut(),0)};
 if code<0{return Err(io::Error::other(format!("NtCreateFile status {code:#010x}")))}
 let file=unsafe{File::from_raw_handle(handle)};validate_dir(&file)?;Ok(file)
}
fn supports_private_runtime(flags:u32,filesystem:&[u16])->bool {flags&0x8!=0&&filesystem.iter().copied().eq("NTFS".encode_utf16())}
fn pin_parent(path:&Path)->io::Result<Vec<File>> {
 for part in path.as_os_str().encode_wide().collect::<Vec<_>>().split(|c|*c==b'/' as u16||*c==b'\\' as u16) {
  if part==[b'.' as u16,b'.' as u16]||part==[b'.' as u16] {return Err(invalid("dot components forbidden"))}
 }
 let mut components=path.components();
 let letter=match components.next(){Some(Component::Prefix(p))=>match p.kind(){Prefix::Disk(letter)=>letter,_=>return Err(invalid("disk path required"))},_=>return Err(invalid("absolute path required"))};
 if !matches!(components.next(),Some(Component::RootDir)){return Err(invalid("absolute path required"))}
 let root=[letter as u16,b':' as u16,b'\\' as u16,0];let drive=unsafe{GetDriveTypeW(root.as_ptr())};
 if !matches!(drive,2|3){return Err(io::Error::new(io::ErrorKind::Unsupported,"runtime path must be on a local volume"))}
 let mut components=components.peekable();
 let root_share=if components.peek().is_some(){SHARE_ANCESTOR}else{SHARE_PRIVATE};
 let handle=unsafe{CreateFileW(root.as_ptr(),ACCESS,root_share,ptr::null_mut(),3,0x0200_0000|OPEN_REPARSE,ptr::null_mut())};
 if handle as isize == -1{return Err(io::Error::last_os_error())}
 let root=unsafe{File::from_raw_handle(handle)};validate_dir(&root)?;
 let mut filesystem=[0u16;32];let mut flags=0u32;
 if unsafe{GetVolumeInformationByHandleW(root.as_raw_handle(),ptr::null_mut(),0,ptr::null_mut(),ptr::null_mut(),&mut flags,filesystem.as_mut_ptr(),filesystem.len() as u32)}==0{return Err(io::Error::last_os_error())}
 let end=filesystem.iter().position(|&c|c==0).ok_or(invalid("invalid filesystem name"))?;
 if !supports_private_runtime(flags,&filesystem[..end]){return Err(io::Error::new(io::ErrorKind::Unsupported,"runtime requires local NTFS with ACL support"))}
 let mut parents=vec![root];while let Some(part)=components.next() {let name=match part{Component::Normal(v)=>v,_=>return Err(invalid("invalid component"))};let share=if components.peek().is_some(){SHARE_ANCESTOR}else{SHARE_PRIVATE};let next=open_dir(parents.last().unwrap(),name,false,ptr::null_mut(),share)?;validate_dir(parents.last().unwrap())?;parents.push(next);}Ok(parents)
}
/// A created directory and its parents remain pinned until socket deletion.
pub struct PrivateTemp { pub path:PathBuf, directory:Option<File>, _parents:Vec<File> }
impl PrivateTemp {
 pub fn new(parent:&Path,name:&OsStr,descriptor:Handle)->io::Result<Self> {
  let parents=pin_parent(parent)?;let directory=open_dir(parents.last().unwrap(),name,true,descriptor,SHARE_PRIVATE)?;
  Ok(Self{path:parent.join(name),directory:Some(directory),_parents:parents})
 }
}
impl Drop for PrivateTemp {fn drop(&mut self){
 // The pinned directory prevents replacing its path before unlinking the socket.
 let _=std::fs::remove_file(self.path.join("model.sock"));
 if let Some(directory)=self.directory.take(){let dispose:u8=1;let _=unsafe{SetFileInformationByHandle(directory.as_raw_handle(),4,(&dispose as *const u8).cast(),1)};drop(directory)}
}}
#[cfg(test)] mod tests {
 use super::*;
 #[test] fn rejects_junction_as_parent_and_preserves_target() {
  let root=std::env::temp_dir().join(format!("atlas-junction-test-{}",std::process::id()));
  std::fs::create_dir(&root).unwrap();let target=root.join("target");std::fs::create_dir(&target).unwrap();let link=root.join("link");
  let status=std::process::Command::new("cmd.exe").arg("/C").arg("mklink").arg("/J").arg(&link).arg(&target).status().unwrap();assert!(status.success());
  let result=pin_parent(&link);assert!(matches!(result,Err(ref e) if e.kind()==io::ErrorKind::PermissionDenied),"junction not rejected");
  assert!(target.is_dir());std::fs::remove_dir(&link).unwrap();std::fs::remove_dir(&target).unwrap();std::fs::remove_dir(&root).unwrap();
 }
 #[test] fn runtime_directory_cannot_be_renamed_and_is_removed_on_drop() {
  let key=super::super::windows_key::KeyFile::new().unwrap();let path=key.dir.clone();let renamed=path.with_extension("renamed");
  assert!(std::fs::rename(&path,&renamed).is_err(),"pinned directory renamed");drop(key);
  assert!(!path.exists(),"runtime directory leaked");assert!(!renamed.exists());
 }
}
#[cfg(test)] mod independent_reparse_test {
 use super::*; use std::os::windows::ffi::OsStrExt;
 #[link(name="kernel32")] unsafe extern "system" {fn DeviceIoControl(h:Handle,code:u32,input:*const c_void,n:u32,out:*mut c_void,outn:u32,bytes:*mut u32,overlap:*mut c_void)->i32;}
 pub(super) fn attempt(dir:&Path,target:&Path)->Result<(),i32> {
  let path:Vec<_>=dir.as_os_str().encode_wide().chain(Some(0)).collect();
  let h=unsafe{CreateFileW(path.as_ptr(),0x4000_0000,7,ptr::null_mut(),3,0x0200_0000|OPEN_REPARSE,ptr::null_mut())};
  if h as isize == -1{return Err(io::Error::last_os_error().raw_os_error().unwrap())}
  let file=unsafe{File::from_raw_handle(h)};
  let prefix=format!("\\??\\{}",target.display());let mut name:Vec<u16>=prefix.encode_utf16().collect();let namebytes=name.len()*2;name.extend([0,0]);let data_len=8+name.len()*2;
  let mut buf=Vec::<u8>::new();buf.extend(0xa000_0003u32.to_le_bytes());buf.extend((data_len as u16).to_le_bytes());buf.extend(0u16.to_le_bytes());buf.extend(0u16.to_le_bytes());buf.extend((namebytes as u16).to_le_bytes());buf.extend(((namebytes+2) as u16).to_le_bytes());buf.extend(0u16.to_le_bytes());for x in name{buf.extend(x.to_le_bytes());}
  let mut bytes=0;let ok=unsafe{DeviceIoControl(file.as_raw_handle(),0x000900a4,buf.as_ptr().cast(),buf.len() as u32,ptr::null_mut(),0,&mut bytes,ptr::null_mut())};if ok==0{Err(io::Error::last_os_error().raw_os_error().unwrap())}else{Ok(())}
 }
 #[test] fn directory_open_cannot_change_to_junction(){
  let base=std::env::temp_dir().join(format!("atlas-socket-race-{}",std::process::id()));std::fs::create_dir(&base).unwrap();let target=base.join("target");let control=base.join("control");std::fs::create_dir(&target).unwrap();std::fs::create_dir(&control).unwrap();
  assert_eq!(attempt(&control,&target),Ok(()),"control must be able to create junction");std::fs::remove_dir(&control).unwrap();
  let key=super::super::windows_key::KeyFile::new().unwrap();let result=attempt(&key.dir,&target);drop(key);std::fs::remove_dir(&target).unwrap();std::fs::remove_dir(&base).unwrap();assert_eq!(result,Err(32),"attacker must be denied with sharing violation");
 }
}

#[cfg(test)] mod sharing_impact_test {
 use super::*;
 #[test] fn a_preexisting_writer_to_parent_prevents_private_dir_creation() {
  let parent=std::env::temp_dir();let name:Vec<u16>=parent.as_os_str().encode_wide().chain(Some(0)).collect();
  let raw=unsafe{CreateFileW(name.as_ptr(),0x4000_0000,7,ptr::null_mut(),3,0x0200_0000|OPEN_REPARSE,ptr::null_mut())};assert_ne!(raw as isize,-1);
  let _hold=unsafe{File::from_raw_handle(raw)};
  let result=super::super::windows_key::KeyFile::new();assert!(result.is_err(),"writer did not prevent runtime creation");
 }
}

#[cfg(test)] mod ancestor_compatibility_test {
 use super::*;
 #[test] fn preexisting_ancestor_writer_allowed_but_reparse_and_removal_denied() {
  let base=std::env::temp_dir().join(format!("atlas-ancestor-test-{}",std::process::id()));std::fs::create_dir(&base).unwrap();
  let outer=base.join("outer");let inner=outer.join("inner");let target=base.join("target");std::fs::create_dir(&outer).unwrap();std::fs::create_dir(&inner).unwrap();std::fs::create_dir(&target).unwrap();
  let wide:Vec<_>=outer.as_os_str().encode_wide().chain(Some(0)).collect();let raw=unsafe{CreateFileW(wide.as_ptr(),0x4000_0000,7,ptr::null_mut(),3,0x0200_0000|OPEN_REPARSE,ptr::null_mut())};assert_ne!(raw as isize,-1);let writer=unsafe{File::from_raw_handle(raw)};
  let pinned=pin_parent(&inner).expect("existing writer on ancestor should not block");
  let error=super::independent_reparse_test::attempt(&outer,&target);assert_eq!(error,Err(145),"non-empty ancestor must reject junction");
  let error=super::independent_reparse_test::attempt(&inner,&target);assert_eq!(error,Err(32),"guarded temp must reject write");
  assert!(std::fs::rename(&inner,outer.join("renamed")).is_err());drop(pinned);drop(writer);
  std::fs::remove_dir(&inner).unwrap();std::fs::remove_dir(&outer).unwrap();std::fs::remove_dir(&target).unwrap();std::fs::remove_dir(&base).unwrap();
 }
}

#[cfg(test)] mod volume_policy_test {
 use super::*;
 #[test] fn refuses_filesystems_without_persistent_acl_support() {
  assert!(supports_private_runtime(0x8,&"NTFS".encode_utf16().collect::<Vec<_>>()));
  assert!(!supports_private_runtime(0,&"NTFS".encode_utf16().collect::<Vec<_>>()));
  assert!(!supports_private_runtime(0x8,&"exFAT".encode_utf16().collect::<Vec<_>>()));
 }
}
