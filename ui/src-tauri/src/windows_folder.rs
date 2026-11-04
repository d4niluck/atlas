use std::{ffi::c_void, ptr};
use std::os::windows::ffi::OsStringExt;

#[repr(C)]
struct Guid { d1:u32, d2:u16, d3:u16, d4:[u8;8] }
const CLSID_FILE_OPEN:Guid=Guid{d1:0xdc1c5a9c,d2:0xe88a,d3:0x4dde,d4:[0xa5,0xa1,0x60,0xf8,0x2a,0x20,0xae,0xf7]};
const IID_FILE_OPEN:Guid=Guid{d1:0xd57c7288,d2:0xd4ad,d3:0x4768,d4:[0xbe,0x02,0x9d,0x96,0x95,0x32,0xd9,0x60]};
const FOS_PICKFOLDERS:u32=0x20;
const FOS_FORCEFILESYSTEM:u32=0x40;
const FOS_NOCHANGEDIR:u32=0x8;
const SIGDN_FILESYSPATH:u32=0x80058000;
const CANCELLED:i32=0x800704c7u32 as i32;
#[link(name="ole32")]
unsafe extern "system" {
 fn CoInitializeEx(reserved:*mut c_void, flags:u32)->i32;
 fn CoUninitialize();
 fn CoCreateInstance(clsid:*const Guid,outer:*mut c_void,context:u32,iid:*const Guid,out:*mut *mut c_void)->i32;
 fn CoTaskMemFree(ptr:*mut c_void);
}
#[repr(C)]
struct IUnknownVtable { query:usize, add_ref:usize, release:unsafe extern "system" fn(*mut c_void)->u32 }
#[repr(C)]
struct DialogVtable {
 base:IUnknownVtable,
 show:unsafe extern "system" fn(*mut c_void,*mut c_void)->i32,
 set_file_types:usize, set_file_type_index:usize, get_file_type_index:usize, advise:usize, unadvise:usize,
 set_options:unsafe extern "system" fn(*mut c_void,u32)->i32,
 get_options:unsafe extern "system" fn(*mut c_void,*mut u32)->i32,
 set_default_folder:usize,set_folder:usize,get_folder:usize,get_current_selection:usize,set_file_name:usize,get_file_name:usize,set_title:usize,set_ok_button_label:usize,set_filename_label:usize,
 get_result:unsafe extern "system" fn(*mut c_void,*mut *mut c_void)->i32,
}
#[repr(C)]
struct ShellItemVtable {base:IUnknownVtable,bind_to_handler:usize,get_parent:usize,get_display_name:unsafe extern "system" fn(*mut c_void,u32,*mut *mut u16)->i32}
struct ComGuard;
impl Drop for ComGuard { fn drop(&mut self){unsafe {CoUninitialize()}} }
struct ComPtr(*mut c_void);
impl Drop for ComPtr {fn drop(&mut self){unsafe{let vt=*(self.0 as *mut *const IUnknownVtable);((*vt).release)(self.0);}}}
struct TaskMem(*mut u16);
impl Drop for TaskMem{fn drop(&mut self){unsafe{CoTaskMemFree(self.0.cast())}}}
fn check(hr:i32)->Result<(),String>{if hr<0{Err(format!("Не удалось открыть выбор папки (код {hr:#x})."))}else{Ok(())}}
fn show_succeeded(hr:i32)->Result<bool,String>{if hr==CANCELLED{Ok(false)}else{check(hr)?;Ok(true)}}
fn selected_path(wide:&[u16])->Result<String,String>{let path=std::ffi::OsString::from_wide(wide).into_string().map_err(|_|"Имя папки не поддерживается.")?;if !std::path::Path::new(&path).is_absolute(){return Err("Не удалось прочитать выбранную папку.".into())}Ok(path)}
fn select_folder_on_sta(owner:isize)->Result<Option<String>,String>{unsafe {
 // A fresh thread ensures the dialog uses STA even when the caller uses another COM mode.
 check(CoInitializeEx(ptr::null_mut(),0x2|0x4))?;let _com=ComGuard;
 let mut raw=ptr::null_mut();check(CoCreateInstance(&CLSID_FILE_OPEN,ptr::null_mut(),1,&IID_FILE_OPEN,&mut raw))?;
 if raw.is_null(){return Err("Не удалось открыть выбор папки.".into())}let dialog=ComPtr(raw);
 let vt=*(raw as *mut *const DialogVtable);let mut flags=0;check(((*vt).get_options)(raw,&mut flags))?;
 check(((*vt).set_options)(raw,flags|FOS_PICKFOLDERS|FOS_FORCEFILESYSTEM|FOS_NOCHANGEDIR))?;
 let hr=((*vt).show)(raw,owner as *mut c_void);if !show_succeeded(hr)?{return Ok(None)}
 let mut item=ptr::null_mut();check(((*vt).get_result)(dialog.0,&mut item))?;
 if item.is_null(){return Err("Не удалось прочитать выбранную папку.".into())}let item=ComPtr(item);
 let ivt=*(item.0 as *mut *const ShellItemVtable);let mut wide=ptr::null_mut();check(((*ivt).get_display_name)(item.0,SIGDN_FILESYSPATH,&mut wide))?;
 if wide.is_null(){return Err("Не удалось прочитать выбранную папку.".into())}let text=TaskMem(wide);
 let len=(0..).find(|&n|*text.0.add(n)==0).ok_or("Некорректный путь.")?;
 let path=selected_path(std::slice::from_raw_parts(text.0,len))?;
 Ok(Some(path))
}}
pub fn select_folder(owner:isize)->Result<Option<String>,String>{std::thread::Builder::new().name("atlas-folder-picker".into()).spawn(move || select_folder_on_sta(owner)).map_err(|_|"Не удалось открыть выбор папки.")?.join().map_err(|_|"Не удалось открыть выбор папки.")?}

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn cancel_is_not_an_error() {assert!(!show_succeeded(CANCELLED).unwrap());assert!(show_succeeded(0).unwrap());assert!(show_succeeded(0x80004005u32 as i32).is_err());}
 #[test] fn selected_folder_must_be_absolute_and_valid_unicode() {
  assert_eq!(selected_path(&"C:\\Работа\\Папка".encode_utf16().collect::<Vec<_>>()).unwrap(),"C:\\Работа\\Папка");
  assert!(selected_path(&"folder".encode_utf16().collect::<Vec<_>>()).is_err());
  assert!(selected_path(&[0xd800]).is_err());
 }
}
