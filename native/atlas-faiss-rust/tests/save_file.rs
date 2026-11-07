use atlas_faiss_probe::Index;
use std::fs::{self, OpenOptions};

#[test]
fn saves_into_existing_file_and_rejects_read_only_file() {
    let dir = std::env::temp_dir().join(format!("atlas-save-file-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("index.faiss");
    let mut file = OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();
    let mut index = Index::new().unwrap();
    let mut a = [0.0; 512]; a[0] = 1.0;
    let mut b = [0.0; 512]; b[1] = 1.0;
    index.add(&[a, b]).unwrap();
    index.save_file(&mut file).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let restored = Index::load(&path).unwrap();
    assert_eq!(restored.count(), 2);
    assert_eq!(restored.reconstruct(0).unwrap(), a);
    assert_eq!(restored.reconstruct(1).unwrap(), b);
    let mut read_only = OpenOptions::new().read(true).open(&path).unwrap();
    assert!(index.save_file(&mut read_only).is_err());
    assert_eq!(Index::load(&path).unwrap().count(), 2);
    fs::remove_dir_all(&dir).unwrap();
}
