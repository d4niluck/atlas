use serde_json::{json,Value};
use std::{path::Path,sync::{Arc,Mutex},os::unix::fs::MetadataExt};
use tiny_http::Request;

pub fn handle(mut req:Request,dir:&Path,resource:&[&str],method:&str,semantic:Option<&crate::semantic::SemanticService>,rag:&Arc<Mutex<crate::RagState>>,limit:usize){
 let page=if resource==["chat","history"]&&method=="GET"{match crate::chat::pagination(req.url()){Ok(v)=>Some(v),Err(e)=>return crate::answer(req,400,json!({"error":e}))}}else{None};
 let input=if method=="POST" {match crate::body(&mut req){Ok(v)=>v,Err(_)=>return crate::answer(req,400,json!({"error":"Некорректный запрос"}))}}else{Value::Null};
 let result=(||->Result<Value,String>{
  let (snapshot,identity,scope)={let _lock=atlas_native_storage_contract::workspace_lock(dir).map_err(|e|e.to_string())?;if !crate::valid_workspace(dir){return Err("Пространство недоступно".into())}let metadata=std::fs::metadata(dir).map_err(|e|e.to_string())?;let identity=(metadata.dev(),metadata.ino());let mut db=rusqlite::Connection::open(dir.join("atlas.sqlite3")).map_err(|e|e.to_string())?;
   if resource==["chat","clear"]&&method=="POST"{atlas_native_storage_contract::chat_state::clear_paged(&mut db,||Ok(()))?;return Ok(json!({"ok":true}))}
   if let Some((before,page_limit))=page{return atlas_native_storage_contract::chat_state::page(&mut db,before,page_limit,||Ok(()))}
   let snapshot=atlas_native_storage_contract::chat_state::context(&mut db,||Ok(()))?;
   if resource!=["chat","send"]||method!="POST"{return Err("Неизвестная операция".into())}
   let ids=match input.get("object_ids"){None|Some(Value::Null)=>None,Some(Value::Array(a))=>Some(a.iter().map(|v|v.as_str().map(str::to_owned).ok_or("Неверный фильтр")).collect::<Result<Vec<_>,_>>()?),_=>return Err("Неверный фильтр".into())};let scope=atlas_native_storage_contract::search_scope(dir,ids).map_err(|e|e.to_string())?;(snapshot,identity,scope)};
  let query=input["query"].as_str().ok_or("Неверный запрос")?;let mut state=crate::chat::State::read(&snapshot)?;state.restrict(json!(scope));let revision=snapshot["revision"].as_i64().ok_or("История недоступна")?;
  atlas_llama_probe::set_search_deadline(std::time::Instant::now()+std::time::Duration::from_secs(300));
  let model={let slot=rag.lock().map_err(|_|"Модель недоступна")?;match &*slot{crate::RagState::Ready(v)=>v.clone(),_=>return Err("Модель ответа не загружена".into())}};let generator=model.acquire()?;let mut search_revision=None;
  let (state,answer,sources)=crate::chat::run(&*generator,&state,query,limit,|q|{let service=semantic.ok_or("Модель поиска не загружена")?;service.ensure(dir)?;let revision=service.revision(dir).ok_or("Индекс ещё не готов")?;if search_revision.is_some_and(|n|n!=revision){return Err("Данные изменились во время поиска".into())}search_revision=Some(revision);let rows=crate::exclude_image_chunks(service.search(dir,q,scope.as_deref())?);crate::chat::answer_search(&*generator,q,&rows)},&mut |stage,text|crate::rag_stream::progress(stage,text))?;drop(generator);
  let _lock=atlas_native_storage_contract::workspace_lock(dir).map_err(|e|e.to_string())?;let metadata=std::fs::metadata(dir).map_err(|e|e.to_string())?;if (metadata.dev(),metadata.ino())!=identity||!crate::valid_workspace(dir){return Err("Пространство изменилось".into())}
  if let Some(expected)=search_revision{if semantic.and_then(|s|s.revision(dir))!=Some(expected){return Err("Данные изменились во время поиска".into())}}
  let ids=match input.get("object_ids"){None|Some(Value::Null)=>None,Some(Value::Array(a))=>Some(a.iter().map(|v|v.as_str().map(str::to_owned).ok_or("Неверный фильтр")).collect::<Result<Vec<_>,_>>()?),_=>return Err("Неверный фильтр".into())};if atlas_native_storage_contract::search_scope(dir,ids).map_err(|e|e.to_string())?!=scope{return Err("Область поиска изменилась".into())}
  let mut db=rusqlite::Connection::open(dir.join("atlas.sqlite3")).map_err(|e|e.to_string())?;let delta=state.value();crate::rag_stream::commit(||atlas_native_storage_contract::chat_state::append(&mut db,revision,&delta["user"],&delta["assistant"],&state.summary,state.summarized as i64,state.context_start as i64,&state.scope,||Ok(()))).map_err(|_|"Поиск остановлен")??;
  Ok(json!({"answer":answer,"sources":sources}))
 })();match result{Ok(v)=>crate::answer(req,200,v),Err(e)=>crate::answer(req,if crate::rag_stream::cancelled(){499}else{400},json!({"error":e}))}
}
