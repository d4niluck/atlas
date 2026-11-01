use atlas_native_storage_contract::{add_file_stream, create_workspace};
use rusqlite::Connection;
use std::{fs, io::{self, Read}, time::{SystemTime, UNIX_EPOCH}};

struct Broken(bool);
impl Read for Broken {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.0 { return Err(io::Error::other("input failed")); }
        self.0 = true;
        out[0] = b'x';
        Ok(1)
    }
}

#[test]
fn failed_add_leaves_no_object_or_partial_file() {
    let root = std::env::temp_dir().join(format!("atlas-add-fail-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    create_workspace(&root).unwrap();
    assert!(add_file_stream(&root, "sample.txt", &mut Broken(false)).is_err());
    let db = Connection::open(root.join("atlas.sqlite3")).unwrap();
    let count: i64 = db.query_row("SELECT COUNT(*) FROM objects", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0);
    assert_eq!(fs::read_dir(root.join("files")).unwrap().count(), 0);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
