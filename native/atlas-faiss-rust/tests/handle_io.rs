use atlas_faiss_probe::Index;
use std::{fs, io::{Seek, SeekFrom}, path::PathBuf};

fn dir() -> PathBuf {
 let p = std::env::temp_dir().join(format!("atlas-faiss-handles-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
 fs::create_dir(&p).unwrap(); p
}
#[test]
fn read_and_write_use_the_open_file_not_its_replaced_path() {
 let d=dir(); let original=d.join("индекс.faiss"); let moved=d.join("original.faiss");
 let mut idx=Index::new().unwrap();let mut v=[0.;512];v[0]=1.;idx.add(&[v]).unwrap();
 let mut file=fs::OpenOptions::new().read(true).write(true).create_new(true).open(&original).unwrap();
 fs::rename(&original,&moved).unwrap();fs::write(&original,b"unrelated").unwrap();
 idx.write_to(&mut file).unwrap();file.rewind().unwrap();let restored=Index::read_from(&mut file).unwrap();
 assert_eq!(restored.count(),1);assert_eq!(fs::read(&original).unwrap(),b"unrelated");drop(file);fs::remove_dir_all(d).unwrap();
}
#[test]
fn failures_are_reported_and_file_position_is_respected() {
 let d=dir();let path=d.join("index"); let idx=Index::new().unwrap();fs::write(&path,b"not an index").unwrap();
 let mut read=fs::File::open(&path).unwrap();assert!(idx.write_to(&mut read).is_err());
 read.seek(SeekFrom::Start(0)).unwrap();assert!(Index::read_from(&mut read).is_err());drop(read);fs::remove_dir_all(d).unwrap();
}
#[test]
fn streams_index_at_current_offset_and_rejects_truncated_data() {
 use std::io::Write;
 let d=dir(); let path=d.join("data");let idx=Index::new().unwrap();
 let mut file=fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();
 file.write_all(b"prefix").unwrap();idx.write_to(&mut file).unwrap();
 let end=file.stream_position().unwrap();file.seek(SeekFrom::Start(6)).unwrap();assert_eq!(Index::read_from(&mut file).unwrap().count(),0);
 file.set_len(end-1).unwrap();file.seek(SeekFrom::Start(6)).unwrap();assert!(Index::read_from(&mut file).is_err());
 drop(file);fs::remove_dir_all(d).unwrap();
}
