//! Prototype: publish an already encrypted, closed SQLite container without resolving destination paths twice.
use std::{ffi::CString,fs::File,io::{Read,Write,Seek,SeekFrom},path::{Component,Path},os::{fd::{AsRawFd,FromRawFd},unix::ffi::OsStrExt}};

pub fn publish_ciphertext(mut source:File, parent:File, name:&str)->Result<(),String>{
 let mut components=Path::new(name).components();let name=match(components.next(),components.next()){(Some(Component::Normal(v)),None)=>CString::new(v.as_bytes()).map_err(|_|"Недопустимое имя")?,_=>return Err("Недопустимое имя".into())};
 let metadata=source.metadata().map_err(|e|e.to_string())?;if !metadata.is_file()||!parent.metadata().map_err(|e|e.to_string())?.is_dir(){return Err("Недопустимый источник или папка".into())}let before=crate::source_stamp(&metadata);let size=metadata.len();source.seek(SeekFrom::Start(0)).map_err(|e|e.to_string())?;let mut stat=std::mem::MaybeUninit::<libc::statvfs>::uninit();if unsafe{libc::fstatvfs(parent.as_raw_fd(),stat.as_mut_ptr())}!=0{return Err("Не удалось проверить свободное место".into())}let stat=unsafe{stat.assume_init()};if (stat.f_bavail as u128)*(stat.f_frsize as u128)<size as u128{return Err("Недостаточно места для переноса".into())}
 let stage=CString::new(format!(".atlas-encrypted-stage-{}",uuid::Uuid::new_v4().simple())).unwrap();let fd=unsafe{libc::openat(parent.as_raw_fd(),stage.as_ptr(),libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL|libc::O_NOFOLLOW|libc::O_CLOEXEC,0o600)};if fd<0{return Err("Не удалось создать зашифрованную копию".into())}let mut out=unsafe{File::from_raw_fd(fd)};
 let copy=(||->Result<(),String>{let mut n=0u64;let mut block=[0u8;65536];loop{let got=source.read(&mut block).map_err(|e|e.to_string())?;if got==0{break}out.write_all(&block[..got]).map_err(|e|e.to_string())?;n+=got as u64;#[cfg(feature="test-hook")] if n==got as u64{test_hook("after_first_block")?;}}if n!=size||crate::source_stamp(&source.metadata().map_err(|e|e.to_string())?)!=before{return Err("Неполная или изменившаяся зашифрованная копия".into())}out.sync_all().map_err(|e|e.to_string())?;#[cfg(feature="test-hook")] test_hook("before_link")?;if unsafe{libc::linkat(parent.as_raw_fd(),stage.as_ptr(),parent.as_raw_fd(),name.as_ptr(),0)}!=0{return Err("Не удалось опубликовать копию".into())}#[cfg(feature="test-hook")] test_hook("after_link")?;parent.sync_all().map_err(|_|"Копия создана, но не подтверждена запись каталога".to_owned())?;Ok(())})();
 unsafe{libc::unlinkat(parent.as_raw_fd(),stage.as_ptr(),0)};
 if copy.is_ok(){parent.sync_all().map_err(|_|"Копия создана, но не подтверждена очистка временного файла".to_owned())?;}
 copy
}

// Test-only barriers use artificial files and are absent from ordinary builds.
#[cfg(feature="test-hook")]
fn test_hook(phase:&str)->Result<(),String>{
 if std::env::var("ATLAS_PUBLICATION_TEST_FAIL").ok().as_deref()==Some(phase){return Err("Искусственная ошибка записи".into())}
 if std::env::var("ATLAS_PUBLICATION_TEST_PHASE").ok().as_deref()==Some(phase){let dir=std::env::var_os("ATLAS_PUBLICATION_TEST_DIR").ok_or("Тестовая папка не задана")?;let dir=Path::new(&dir);std::fs::write(dir.join("ready"),phase).map_err(|e|e.to_string())?;while !dir.join("resume").exists(){std::thread::sleep(std::time::Duration::from_millis(10));}}
 Ok(())
}
