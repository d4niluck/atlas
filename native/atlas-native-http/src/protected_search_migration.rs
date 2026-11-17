use atlas_sqlcipher_proof::{Vault,search_index_name,search_stage::build_search_stage_local_with_generation};

/// Called with a dedicated Vault; the editor's mutex is held only by `commit`.
pub fn prepare<G>(source:&mut Vault,password:&str,verify:impl Fn()->Result<(),String>,guard:impl Fn()->Result<G,String>,verify_parent:impl Fn(&std::fs::File)->Result<(),String>,phase:impl Fn(&str))->Result<(atlas_sqlcipher_proof::search_stage::SearchStage,atlas_sqlcipher_proof::RecordedSearchCopy,atlas_sqlcipher_proof::SearchBackup),String>{
 let path=source.workspace_path()?;let dir=path.parent().ok_or("Папка недоступна")?;let parent=dir.parent().ok_or("Папка недоступна")?;
 verify()?;let check=Vault::open(&path,password)?;drop(check);
 let generation=uuid::Uuid::new_v4().simple().to_string();let target=dir.join(search_index_name(&generation)?);
 phase("Подготавливаем журнал поиска");source.enable_search_journal_guarded(||verify())?;source.begin_search_migration(&generation,&target,||{let lock=guard()?;verify()?;Ok(lock)})?;
 phase("Подготавливаем индекс");let mut stage=build_search_stage_local_with_generation(source,password,Some(&generation),||verify())?;
 loop{verify()?;let pending=source.pending_search_updates(1)?;if pending.is_empty(){break}if stage.replay_pending_guarded(source,password,128,||verify())?==0{return Err("Поиск изменяется; повторите попытку".into())}}
 phase("Создаём защищённую копию");let backup_path=parent.join(format!(".atlas-search-backup-{generation}.atlas"));let mut backup=source.prepare_search_backup_guarded(&backup_path,password,||verify())?;
 phase("Сохраняем индекс");
 #[cfg(feature="test-hook")] if let Some(path)=std::env::var_os("ATLAS_TEST_SEARCH_COPY_BARRIER"){let dir=std::path::Path::new(&path);std::fs::write(dir.join("ready"),b"ready").map_err(|e|e.to_string())?;while !dir.join("resume").exists(){verify()?;std::thread::sleep(std::time::Duration::from_millis(10));}}
 let copy=source.prepare_recorded_search_copy(&mut stage,&mut backup,&target,||verify(),||guard(),|dir|verify_parent(dir))?;
 Ok((stage,copy,backup))
}

#[cfg(all(test,feature="test-hook"))] mod tests{
 use super::*;
 #[test] fn prepares_and_activates_encrypted_search(){let root=std::env::temp_dir().join(format!("atlas-search-job-{}",uuid::Uuid::new_v4()));let dir=root.join("workspace");std::fs::create_dir_all(&dir).unwrap();let mut v=Vault::create(&dir.join("workspace.atlas"),"secret").unwrap();v.put_note("n","title","sample").unwrap();let (stage,copy,backup)=prepare(&mut v,"secret",||Ok(()),||Ok(()),|_|Ok(()),|_|{}).unwrap();v.commit_recorded_search_cutover(&stage,&copy,&backup,"secret",||Ok(())).unwrap();assert_eq!(v.search("sample").unwrap(),vec!["n"]);drop(v);drop(stage);std::fs::remove_dir_all(root).unwrap();}
}
