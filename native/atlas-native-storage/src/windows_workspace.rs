#![cfg(windows)]
use std::{ffi::{c_void,OsStr,OsString},fs::File,io,mem::{size_of,zeroed},os::windows::{ffi::{OsStrExt,OsStringExt},io::{AsRawHandle,FromRawHandle}},path::{Component,Path,Prefix},ptr};
type Handle=*mut c_void;
const SHARE_DIRECTORY:u32=1;
const SHARE_ANCESTOR:u32=3;
const SHARE_LOCK:u32=3;
const READ_ATTR:u32=0x80;
const TRAVERSE:u32=0x20;
const SYNCHRONIZE:u32=0x0010_0000;
const OPEN_REPARSE:u32=0x0020_0000;
const BACKUP_SEMANTICS:u32=0x0200_0000;
const ATTR_REPARSE:u32=0x400;
const ATTR_DIRECTORY:u32=0x10;
#[repr(C)] struct AttrTag {attrs:u32,tag:u32}
#[repr(C)] #[derive(PartialEq, Eq)] struct FileIdentity {volume:u64, id:[u8;16]}
#[repr(C)] struct StandardInfo { allocation:i64, length:i64, links:u32, delete_pending:u8, directory:u8 }
#[repr(C)] struct UString {len:u16,max:u16,buf:*mut u16}
#[repr(C)] struct ObjAttr {len:u32,root:Handle,name:*mut UString,attrs:u32,security:Handle,qos:Handle}
#[repr(C)] struct IoStatus {status:usize,info:usize}
#[repr(C)] struct Overlapped {internal:usize,high:usize,offset:u32,offset_high:u32,event:Handle}
#[link(name="kernel32")] unsafe extern "system" {
 fn CreateFileW(name:*const u16,access:u32,share:u32,security:Handle,disposition:u32,flags:u32,template:Handle)->Handle;
 fn GetFileInformationByHandleEx(file:Handle,class:u32,info:*mut c_void,size:u32)->i32;
 fn GetDriveTypeW(root:*const u16)->u32;
 fn GetFinalPathNameByHandleW(file:Handle,path:*mut u16,length:u32,flags:u32)->u32;
 fn GetVolumeInformationByHandleW(file:Handle,name:*mut u16,name_len:u32,serial:*mut u32,max_component:*mut u32,flags:*mut u32,filesystem:*mut u16,filesystem_len:u32)->i32;
 fn LockFileEx(file:Handle,flags:u32,reserved:u32,low:u32,high:u32,overlap:*mut Overlapped)->i32;
}
#[link(name="ntdll")] unsafe extern "system" {
 fn NtCreateFile(file:*mut Handle,access:u32,attr:*mut ObjAttr,io:*mut IoStatus,size:*mut i64,file_attr:u32,share:u32,disposition:u32,options:u32,ea:*mut c_void,ea_len:u32)->i32;
}
fn invalid(msg:&'static str)->io::Error {io::Error::new(io::ErrorKind::InvalidInput,msg)}
fn bad_reparse()->io::Error {io::Error::new(io::ErrorKind::PermissionDenied,"reparse point forbidden")}
fn validate(file:&File,directory:bool)->io::Result<()> {
 let mut info=AttrTag{attrs:0,tag:0};
 if unsafe {GetFileInformationByHandleEx(file.as_raw_handle(),9,(&mut info as *mut AttrTag).cast(),size_of::<AttrTag>() as u32)}==0{return Err(io::Error::last_os_error())}
 if info.attrs&ATTR_REPARSE!=0 {return Err(bad_reparse())}
 if (info.attrs&ATTR_DIRECTORY!=0)!=directory {return Err(invalid("unexpected object type"))}
 Ok(())
}
fn open_relative(parent:&File,name:&OsStr,directory:bool,share:u32)->io::Result<File>{
 let mut name:Vec<u16>=name.encode_wide().collect();
 if name.is_empty()||name.len()>255||name.iter().any(|c|matches!(*c,0|47|58|92))||matches!(name.last(),Some(32|46)){return Err(invalid("invalid path component"))}
 let len:u16=(name.len()*2).try_into().map_err(|_|invalid("component too long"))?;
 let mut unicode=UString{len,max:len,buf:name.as_mut_ptr()};
 let mut attr=ObjAttr{len:size_of::<ObjAttr>() as u32,root:parent.as_raw_handle(),name:&mut unicode,attrs:0x40,security:ptr::null_mut(),qos:ptr::null_mut()};
 let mut status=IoStatus{status:0,info:0}; let mut handle:Handle=ptr::null_mut();
 let access=if directory{READ_ATTR|TRAVERSE|SYNCHRONIZE}else{0xc000_0000|SYNCHRONIZE};
 let disposition=if directory{1}else{3}; let options=0x20|(if directory{1}else{0x40})|OPEN_REPARSE;
 let result=unsafe{NtCreateFile(&mut handle,access,&mut attr,&mut status,ptr::null_mut(),0x80,share,disposition,options,ptr::null_mut(),0)};
 if result<0{return Err(io::Error::other(format!("NtCreateFile status {result:#010x}")))}
 let file=unsafe{File::from_raw_handle(handle)};validate(&file,directory)?;Ok(file)
}
/// Holds all directory handles and the exclusive session lock. Experimental: not integrated.
pub struct WorkspaceLock { _lock:File, _parents:Vec<File>, _incoming:Option<File>, bound_path:std::path::PathBuf, is_session:bool }
impl WorkspaceLock {
 pub fn bound_path(&self)->&Path { &self.bound_path }
 /// Borrow the pinned workspace directory; consumers must retain the session for the entire operation.
 pub fn root_handle(&self)->io::Result<std::os::windows::io::BorrowedHandle<'_>> {
  use std::os::windows::io::AsHandle;
  if !self.is_session{return Err(invalid("session required"))}
  Ok(self._parents.last().ok_or(invalid("workspace root unavailable"))?.as_handle())
 }
 pub fn operation_lock(&self)->io::Result<WorkspaceOperation<'_>> {
  if !self.is_session {return Err(invalid("operation guard cannot acquire another operation lock"))}
  let root=self._parents.last().ok_or(invalid("missing workspace handle"))?;
  let lock=acquire_lock_file(root,".atlas-operation.lock",true)?;
  Ok(WorkspaceOperation{_lock:lock,session:self})
 }

}
pub struct WorkspaceOperation<'a> { _lock:File, session:&'a WorkspaceLock }
impl WorkspaceOperation<'_> { pub fn bound_path(&self)->&Path { self.session.bound_path() } }
fn handle_bound_path(file:&File)->io::Result<std::path::PathBuf> {
 const VOLUME_NAME_NT:u32=2;
 let needed=unsafe{GetFinalPathNameByHandleW(file.as_raw_handle(),ptr::null_mut(),0,VOLUME_NAME_NT)};
 if needed==0{return Err(io::Error::last_os_error())}
 let mut raw=vec![0u16;needed as usize+1];
 let n=unsafe{GetFinalPathNameByHandleW(file.as_raw_handle(),raw.as_mut_ptr(),raw.len() as u32,VOLUME_NAME_NT)};
 if n==0{return Err(io::Error::last_os_error())} if n as usize>=raw.len(){return Err(invalid("path changed while resolving"))}
 raw.truncate(n as usize);
 let prefix:Vec<u16>=r"\\?\GLOBALROOT".encode_utf16().collect();
 // Network redirects (e.g. \Device\Mup) are outside the tested local NTFS contract.
 let local:Vec<u16>=r"\Device\HarddiskVolume".encode_utf16().collect();
 if !raw.starts_with(&local) {return Err(io::Error::new(io::ErrorKind::Unsupported,"unsupported device path"))}
 let mut tail=raw[local.len()..].iter().copied();
 let mut digits=0usize;let mut delimiter=None;
 for c in &mut tail {if (48..=57).contains(&c){digits+=1}else{delimiter=Some(c);break}}
 if digits==0||delimiter.is_some_and(|c|c!=b'\\' as u16){return Err(io::Error::new(io::ErrorKind::Unsupported,"unsupported device path"))}
 let result=prefix.into_iter().chain(raw).collect::<Vec<_>>();
 Ok(std::path::PathBuf::from(OsString::from_wide(&result)))
}
pub fn lock_session(path:&Path)->io::Result<WorkspaceLock>{ lock_impl(path, ".atlas-session.lock", false, None) }
/// Reacquire child-owned session from a retained directory handle, validating identity.
/// The caller is responsible for authenticating the source of the handle.
pub fn lock_session_from_handle(directory:File)->io::Result<WorkspaceLock>{ lock_session_from_handle_inner(directory,||{}) }
#[cfg(feature="test-hook")]
pub fn lock_session_from_handle_with_hook(directory:File,hook:impl FnOnce())->io::Result<WorkspaceLock>{lock_session_from_handle_inner(directory,hook)}
fn lock_session_from_handle_inner(directory:File,hook:impl FnOnce())->io::Result<WorkspaceLock>{
 validate(&directory,true)?;
 let original=file_identity(&directory)?;
 let bound=handle_bound_path(&directory)?;
 hook();
 let mut session=lock_impl(&bound,".atlas-session.lock",false,Some(&original))?;
 if file_identity(session._parents.last().ok_or(invalid("missing workspace handle"))?)? != original {
  return Err(io::Error::new(io::ErrorKind::PermissionDenied,"workspace identity changed"))
 }
 // Retain the transferred handle until the worker session is closed.
 session._incoming=Some(directory);
 Ok(session)
}

pub fn lock_operation(path:&Path)->io::Result<WorkspaceLock>{ lock_impl(path, ".atlas-operation.lock", true, None) }
pub fn lock_workspace(path:&Path)->io::Result<WorkspaceLock>{lock_session(path)}
fn file_identity(file:&File)->io::Result<FileIdentity>{
 let mut info=FileIdentity{volume:0,id:[0;16]};
 if unsafe {GetFileInformationByHandleEx(file.as_raw_handle(),18,(&mut info as *mut FileIdentity).cast(),size_of::<FileIdentity>() as u32)}==0{return Err(io::Error::last_os_error())}
 Ok(info)
}
fn current_directory_handle()->io::Result<File>{
 let name:[u16;2]=[46,0];
 let raw=unsafe{CreateFileW(name.as_ptr(),READ_ATTR|TRAVERSE|SYNCHRONIZE,SHARE_DIRECTORY,ptr::null_mut(),3,BACKUP_SEMANTICS|OPEN_REPARSE,ptr::null_mut())};
 if raw as isize == -1{return Err(io::Error::last_os_error())}
 let file=unsafe{File::from_raw_handle(raw)};validate(&file,true)?;Ok(file)
}
fn lock_impl(path:&Path,lock_name:&str,wait:bool,expected:Option<&FileIdentity>)->io::Result<WorkspaceLock>{
 let (actual_path,cwd_guard)=if path==Path::new(".") {
  let current=current_directory_handle()?;
  (std::env::current_dir()?,Some(current))
 }else{(path.to_path_buf(),None)};
 let path=actual_path.as_path();
 // Reject traversal before `components` can normalize it.
 for part in path.as_os_str().encode_wide().collect::<Vec<_>>().split(|c|*c==b'/' as u16||*c==b'\\' as u16) {
  if part==[b'.' as u16,b'.' as u16]||part==[b'.' as u16] {return Err(invalid("dot components forbidden"))}
 }
 let wide:Vec<u16>=path.as_os_str().encode_wide().collect();
 let global_prefix:Vec<u16>=r"\\?\GLOBALROOT\Device\HarddiskVolume".encode_utf16().collect();
 let (root,components,check_drive)=if wide.starts_with(&global_prefix){
  let mut cursor=global_prefix.len();let first=cursor;
  while cursor<wide.len()&&(48..=57).contains(&wide[cursor]){cursor+=1}
  if cursor==first || (cursor<wide.len()&&wide[cursor]!=b'\\' as u16){return Err(invalid("invalid volume path"))}
  let mut root=wide[..cursor].to_vec();root.push(b'\\' as u16);root.push(0);
  let mut names=Vec::new();
  for name in wide[cursor..].split(|&c|c==b'\\' as u16).filter(|n|!n.is_empty()){names.push(OsString::from_wide(name))}
  (root,names,false)
 }else{
  let mut components=path.components();
  let letter=match components.next(){Some(Component::Prefix(p))=>match p.kind(){Prefix::Disk(letter)|Prefix::VerbatimDisk(letter)=>letter,_=>return Err(invalid("disk path required"))},_=>return Err(invalid("absolute path required"))};
  if !matches!(components.next(),Some(Component::RootDir)){return Err(invalid("absolute path required"))}
  let mut names=Vec::new();for part in components{match part{Component::Normal(name)=>names.push(name.to_os_string()),_=>return Err(invalid("invalid component"))}}
  (vec![letter as u16,b':' as u16,b'\\' as u16,0],names,true)
 };
 if check_drive {let drive_type=unsafe{GetDriveTypeW(root.as_ptr())};if !matches!(drive_type,2|3){return Err(io::Error::new(io::ErrorKind::Unsupported,"unsupported drive type"))}}
 let mut components=components.iter().peekable();
 let root_share=if components.peek().is_some(){SHARE_ANCESTOR}else{SHARE_DIRECTORY};
 let handle=unsafe{CreateFileW(root.as_ptr(),READ_ATTR|TRAVERSE|SYNCHRONIZE,root_share,ptr::null_mut(),3,BACKUP_SEMANTICS|OPEN_REPARSE,ptr::null_mut())};
 if handle as isize == -1{return Err(io::Error::last_os_error())}
 let root=unsafe{File::from_raw_handle(handle)};validate(&root,true)?;
 // Only NTFS has been tested against directory reparse mutations and lock sharing.
 let mut filesystem=[0u16;32];
 if unsafe{GetVolumeInformationByHandleW(root.as_raw_handle(),ptr::null_mut(),0,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),filesystem.as_mut_ptr(),filesystem.len() as u32)}==0{return Err(io::Error::last_os_error())}
 let end=filesystem.iter().position(|&c|c==0).ok_or(invalid("invalid filesystem name"))?;
 if !filesystem[..end].iter().copied().eq("NTFS".encode_utf16()) {return Err(io::Error::new(io::ErrorKind::Unsupported,"unsupported filesystem"))}
 let mut parents=vec![root];
 while let Some(name)=components.next() {let share=if components.peek().is_some(){SHARE_ANCESTOR}else{SHARE_DIRECTORY};let next=open_relative(parents.last().unwrap(),name,true,share)?;parents.push(next);}
 if let Some(current)=cwd_guard {
  if file_identity(&current)?!=file_identity(parents.last().unwrap())? {return Err(io::Error::new(io::ErrorKind::PermissionDenied,"current directory changed"))}
  parents.push(current);
 }
 let bound_path=handle_bound_path(parents.last().unwrap())?;
 // Compare the retained source before creating or opening the lock in the resolved directory.
 if expected.is_some_and(|id|file_identity(parents.last().unwrap()).map_or(true,|actual|actual!=*id)) {return Err(io::Error::new(io::ErrorKind::PermissionDenied,"workspace identity changed"))}
 let file=acquire_lock_file(parents.last().unwrap(),lock_name,wait)?;
 Ok(WorkspaceLock{_lock:file,_parents:parents,_incoming:None,bound_path,is_session:!wait})
}
fn acquire_lock_file(root:&File,lock_name:&str,wait:bool)->io::Result<File> {
 let file=open_relative(root,OsStr::new(lock_name),false,SHARE_LOCK)?;
 let mut standard:StandardInfo=unsafe{zeroed()};
 if unsafe{GetFileInformationByHandleEx(file.as_raw_handle(),1,(&mut standard as *mut StandardInfo).cast(),size_of::<StandardInfo>() as u32)}==0{return Err(io::Error::last_os_error())}
 if standard.links!=1{return Err(io::Error::new(io::ErrorKind::PermissionDenied,"hard link forbidden"))}
 let mut overlap:Overlapped=unsafe{zeroed()};if unsafe{LockFileEx(file.as_raw_handle(),if wait {2}else{3},0,1,0,&mut overlap)}==0 {let e=io::Error::last_os_error();if e.raw_os_error()==Some(33){return Err(io::Error::new(io::ErrorKind::WouldBlock,"workspace session locked"))}return Err(e)}
 Ok(file)
}

/// A read-only file and its checked parent directories, scoped to an open workspace.
pub struct WorkspaceReadFile<'a> { file:File, _parents:Vec<File>, _session:&'a WorkspaceLock }
impl WorkspaceReadFile<'_> { pub fn file_mut(&mut self)->&mut File { &mut self.file } }
impl WorkspaceOperation<'_> {
 pub fn open_read<'a>(&'a self,relative:&Path)->io::Result<WorkspaceReadFile<'a>> {self.session.open_read_inner(relative)}
}
impl WorkspaceLock {
 fn open_read_inner<'a>(&'a self,relative:&Path)->io::Result<WorkspaceReadFile<'a>> {
  let mut parts=relative.components().peekable();
  let mut parents=Vec::new();
  while let Some(part)=parts.next() {
   let name=match part {Component::Normal(name)=>name,_=>return Err(invalid("unsafe workspace path"))};
   if !name.to_str().is_some_and(super::windows_filename::valid_windows_filename) {return Err(invalid("unsafe workspace name"))}
   let parent=parents.last().or_else(||self._parents.last()).ok_or(invalid("workspace root unavailable"))?;
   if parts.peek().is_some(){parents.push(open_relative(parent,name,true,SHARE_DIRECTORY)?);continue}
   let file=open_existing_file_readonly(parent,name)?;
   return Ok(WorkspaceReadFile{file,_parents:parents,_session:self});
  }
  Err(invalid("empty workspace path"))
 }
}
fn open_existing_file_readonly(parent:&File,name:&OsStr)->io::Result<File>{
 let mut name:Vec<u16>=name.encode_wide().collect();
 if name.is_empty()||name.len()>255{return Err(invalid("invalid filename"))}
 let len:u16=(name.len()*2).try_into().map_err(|_|invalid("component too long"))?;
 let mut unicode=UString{len,max:len,buf:name.as_mut_ptr()};
 let mut attr=ObjAttr{len:size_of::<ObjAttr>() as u32,root:parent.as_raw_handle(),name:&mut unicode,attrs:0x40,security:ptr::null_mut(),qos:ptr::null_mut()};
 let mut status=IoStatus{status:0,info:0};let mut handle:Handle=ptr::null_mut();
 // No share-write/delete: reject mutation while a consumer reads the pinned file.
 let result=unsafe{NtCreateFile(&mut handle,0x8000_0000|SYNCHRONIZE,&mut attr,&mut status,ptr::null_mut(),0x80,1,1,0x20|0x40|OPEN_REPARSE,ptr::null_mut(),0)};
 if result<0{return Err(io::Error::other(format!("NtCreateFile status {result:#010x}")))}
 let file=unsafe{File::from_raw_handle(handle)};validate(&file,false)?;
 let mut standard:StandardInfo=unsafe{zeroed()};
 if unsafe{GetFileInformationByHandleEx(file.as_raw_handle(),1,(&mut standard as *mut StandardInfo).cast(),size_of::<StandardInfo>() as u32)}==0{return Err(io::Error::last_os_error())}
 if standard.links!=1{return Err(io::Error::new(io::ErrorKind::PermissionDenied,"hard link forbidden"))}
 Ok(file)
}
