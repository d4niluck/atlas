use atlas_sqlcipher_proof::Vault;
#[test]
fn replacing_image_changes_semantic_fingerprint() {
 let root=std::env::temp_dir().join(format!("atlas-image-fingerprint-{}",uuid::Uuid::new_v4()));
 std::fs::create_dir(&root).unwrap();
 let mut vault=Vault::create(&root.join("workspace.atlas"),"test-password").unwrap();
 vault.put_file("image","sample.png",&b"first image"[..],None).unwrap();
 let first=vault.semantic_fingerprint(true).unwrap();
 let text_only=vault.semantic_fingerprint(false).unwrap();
 vault.put_file("image","sample.png",&b"other image"[..],None).unwrap();
 assert_ne!(first,vault.semantic_fingerprint(true).unwrap());
 assert_eq!(text_only,vault.semantic_fingerprint(false).unwrap());
 drop(vault);std::fs::remove_dir_all(root).unwrap();
}
