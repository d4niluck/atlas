use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};

type Result<T> = std::result::Result<T, String>;

pub fn empty() -> Value { json!({"messages":[],"summary":"","summarized":0,"context_start":0}) }

fn validate(state: &Value) -> Result<()> {
 let messages=state.get("messages").and_then(Value::as_array).ok_or("invalid chat messages")?;
 let summarized=state.get("summarized").and_then(Value::as_u64).filter(|n| *n <= messages.len() as u64).ok_or("invalid chat summary")?;
 if !state.get("summary").is_some_and(Value::is_string) || !state.get("context_start").and_then(Value::as_u64).is_some_and(|n| n <= summarized) { return Err("invalid chat summary".into()); }
 for item in messages {
  if !matches!(item.get("role").and_then(Value::as_str),Some("user"|"assistant")) || !item.get("content").is_some_and(Value::is_string) { return Err("invalid chat message".into()); }
 }
 Ok(())
}

pub fn get(db: &Connection) -> Result<Value> {
 let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='chat_state')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
 if !exists {return Ok(json!({"state":empty(),"revision":0}));}
 let row:Option<(String,i64)>=db.query_row("SELECT state,revision FROM chat_state WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
 match row {None=>Ok(json!({"state":empty(),"revision":0})),Some((raw,revision))=>{if revision<0{return Err("invalid chat revision".into())}let state:Value=serde_json::from_str(&raw).map_err(|_|"invalid chat state")?;validate(&state)?;Ok(json!({"state":state,"revision":revision}))}}
}

pub fn set(db:&mut Connection, expected_revision:i64, state:&Value, before_commit:impl FnOnce()->Result<()>) -> Result<i64> {
 validate(state)?;
 let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
 tx.execute_batch("CREATE TABLE IF NOT EXISTS chat_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1), state TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0));").map_err(|e|e.to_string())?;
 let previous=get(&tx)?["revision"].as_i64().ok_or("invalid chat revision")?;
 if previous!=expected_revision{return Err("chat changed".into())}
 let revision=previous.checked_add(1).ok_or("chat revision exhausted")?;
 tx.execute("INSERT INTO chat_state(singleton,state,revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET state=excluded.state, revision=excluded.revision",params![state.to_string(),revision]).map_err(|e|e.to_string())?;
 before_commit()?;
 tx.commit().map_err(|e|e.to_string())?;
 Ok(revision)
}

pub fn clear(db:&mut Connection, before_commit:impl FnOnce()->Result<()>) -> Result<i64> {
 let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
 tx.execute_batch("CREATE TABLE IF NOT EXISTS chat_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1), state TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0));").map_err(|e|e.to_string())?;
 let previous=get(&tx)?;
 let revision=previous["revision"].as_i64().ok_or("invalid chat revision")?.checked_add(1).ok_or("chat revision exhausted")?;
 let mut state=previous["state"].clone();
 let len=state["messages"].as_array().ok_or("invalid chat messages")?.len();
 state["context_start"]=json!(len);state["summarized"]=json!(len);state["summary"]=json!("");
 tx.execute("INSERT INTO chat_state(singleton,state,revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET state=excluded.state, revision=excluded.revision",params![state.to_string(),revision]).map_err(|e|e.to_string())?;
 before_commit()?;
 tx.commit().map_err(|e|e.to_string())?;
 Ok(revision)
}

// Paged storage keeps old messages out of both the model prompt and the next write.
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CONTEXT_BYTES: usize = 4 * 1024 * 1024;

fn table_exists(db:&Connection,name:&str)->Result<bool>{db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",[name],|r|r.get(0)).map_err(|e|e.to_string())}
fn schema(db:&Connection)->Result<()> {
 db.execute_batch("CREATE TABLE IF NOT EXISTS chat_messages(seq INTEGER PRIMARY KEY AUTOINCREMENT,message TEXT NOT NULL); CREATE TABLE IF NOT EXISTS chat_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),summary TEXT NOT NULL,summarized_seq INTEGER NOT NULL,context_start_seq INTEGER NOT NULL,scope TEXT NOT NULL,revision INTEGER NOT NULL);").map_err(|e|e.to_string())
}
fn valid_message(message:&Value)->Result<()> {
 if !matches!(message.get("role").and_then(Value::as_str),Some("user"|"assistant"))||!message.get("content").is_some_and(Value::is_string){return Err("invalid chat message".into())}
 if message.to_string().len()>MAX_MESSAGE_BYTES{return Err("chat message exceeds size limit".into())}Ok(())
}
fn init(db:&Connection)->Result<()> {
 if table_exists(db,"chat_meta")? {if !table_exists(db,"chat_messages")?{return Err("invalid chat storage".into())}return Ok(())}
 let old=get(db)?;let state=&old["state"];let revision=old["revision"].as_i64().ok_or("invalid chat revision")?;
 schema(db)?;
 let messages=state["messages"].as_array().ok_or("invalid chat state")?;
 for (i,message) in messages.iter().enumerate() {valid_message(message)?;db.execute("INSERT INTO chat_messages(seq,message) VALUES(?1,?2)",params![(i+1) as i64,message.to_string()]).map_err(|e|e.to_string())?;}
 db.execute("INSERT INTO chat_meta VALUES(1,?1,?2,?3,?4,?5)",params![state["summary"].as_str().ok_or("invalid summary")?,state["summarized"].as_i64().ok_or("invalid summary")?,state["context_start"].as_i64().ok_or("invalid context")?,state.get("scope").unwrap_or(&Value::Null).to_string(),revision]).map_err(|e|e.to_string())?;
 db.execute_batch("DROP TABLE IF EXISTS chat_state").map_err(|e|e.to_string())?;Ok(())
}
fn row_value(seq:i64,raw:String)->Result<Value>{let mut v:Value=serde_json::from_str(&raw).map_err(|_|"invalid chat message")?;valid_message(&v)?;v["seq"]=json!(seq);Ok(v)}
fn meta(db:&Connection)->Result<(String,i64,i64,Value,i64,i64)> {
 let (summary,summarized,start,raw,revision):(String,i64,i64,String,i64)=db.query_row("SELECT summary,summarized_seq,context_start_seq,scope,revision FROM chat_meta WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|e|e.to_string())?;
 let last:i64=db.query_row("SELECT COALESCE(MAX(seq),0) FROM chat_messages",[],|r|r.get(0)).map_err(|e|e.to_string())?;
 if revision<0||start<0||start>summarized||summarized>last||summary.len()>MAX_CONTEXT_BYTES{return Err("invalid chat state".into())}
 let scope:Value=serde_json::from_str(&raw).map_err(|_|"invalid chat scope")?;Ok((summary,summarized,start,scope,revision,last))
}

/// Read only the unsummarized context, with an explicit size limit.
pub fn context(db:&mut Connection,guard:impl FnOnce()->Result<()>)->Result<Value>{
 let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;init(&tx)?;
 let(summary,summarized,start,scope,revision,last)=meta(&tx)?;
 let mut messages=Vec::new();let mut bytes=summary.len();
 {let mut st=tx.prepare("SELECT seq,message FROM chat_messages WHERE seq>?1 ORDER BY seq").map_err(|e|e.to_string())?;let mut rows=st.query([summarized]).map_err(|e|e.to_string())?;
 while let Some(row)=rows.next().map_err(|e|e.to_string())?{let seq:i64=row.get(0).map_err(|e|e.to_string())?;let raw:String=row.get(1).map_err(|e|e.to_string())?;bytes=bytes.saturating_add(raw.len());if bytes>MAX_CONTEXT_BYTES{return Err("chat context exceeds size limit".into())}messages.push(row_value(seq,raw)?);}}
 let last_assistant_sources=if last>start {let raw:Option<String>=tx.query_row("SELECT message FROM chat_messages WHERE seq=?1",[last],|r|r.get(0)).optional().map_err(|e|e.to_string())?;match raw{Some(s)=>{let v=row_value(last,s)?;if v["role"]=="assistant"{v.get("sources").cloned().unwrap_or(json!([]))}else{json!([])}},None=>json!([])}}else{json!([])};
 guard()?;tx.commit().map_err(|e|e.to_string())?;Ok(json!({"messages":messages,"summary":summary,"summarized_seq":summarized,"context_start_seq":start,"scope":scope,"revision":revision,"last_message_seq":last,"last_assistant_sources":last_assistant_sources}))
}

/// Latest first on disk, chronological in the returned page. The cursor excludes itself.
pub fn page(db:&mut Connection,before:Option<i64>,limit:usize,guard:impl FnOnce()->Result<()>)->Result<Value>{
 if before.is_some_and(|n|n<=0)||!(1..=100).contains(&limit){return Err("invalid chat page".into())}
 let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;init(&tx)?;
 let mut messages=Vec::<Value>::new();let mut bytes=0usize;let mut more=false;
 {let mut st=tx.prepare("SELECT seq,message FROM chat_messages WHERE (?1 IS NULL OR seq<?1) ORDER BY seq DESC LIMIT ?2").map_err(|e|e.to_string())?;let mut rows=st.query(params![before,(limit+1) as i64]).map_err(|e|e.to_string())?;
 while let Some(row)=rows.next().map_err(|e|e.to_string())?{let seq:i64=row.get(0).map_err(|e|e.to_string())?;let raw:String=row.get(1).map_err(|e|e.to_string())?;if messages.len()==limit||bytes.saturating_add(raw.len())>MAX_PAGE_BYTES {more=true;break}bytes+=raw.len();messages.push(row_value(seq,raw)?);}}
 if messages.is_empty()&&more{return Err("chat message exceeds page limit".into())}
 let next_before=if more {messages.last().and_then(|v|v["seq"].as_i64())}else{None};messages.reverse();
 guard()?;tx.commit().map_err(|e|e.to_string())?;Ok(json!({"messages":messages,"next_before":next_before}))
}

pub fn append(db:&mut Connection,expected_revision:i64,user:&Value,assistant:&Value,summary:&str,summarized_seq:i64,context_start_seq:i64,scope:&Value,guard:impl FnOnce()->Result<()>)->Result<i64>{
 valid_message(user)?;valid_message(assistant)?;if user["role"]!="user"||assistant["role"]!="assistant"||summary.len()>MAX_CONTEXT_BYTES{return Err("invalid chat state".into())}
 let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;init(&tx)?;let(_,_,_,_,revision,last)=meta(&tx)?;
 if revision!=expected_revision{return Err("chat changed".into())}if context_start_seq<0||context_start_seq>summarized_seq||summarized_seq>last{return Err("invalid chat summary".into())}
 let next=revision.checked_add(1).ok_or("chat revision exhausted")?;
 tx.execute("INSERT INTO chat_messages(message) VALUES(?1)",[user.to_string()]).map_err(|e|e.to_string())?;
 tx.execute("INSERT INTO chat_messages(message) VALUES(?1)",[assistant.to_string()]).map_err(|e|e.to_string())?;
 tx.execute("UPDATE chat_meta SET summary=?1,summarized_seq=?2,context_start_seq=?3,scope=?4,revision=?5 WHERE singleton=1",params![summary,summarized_seq,context_start_seq,scope.to_string(),next]).map_err(|e|e.to_string())?;
 guard()?;tx.commit().map_err(|e|e.to_string())?;Ok(next)
}

pub fn clear_paged(db:&mut Connection,guard:impl FnOnce()->Result<()>)->Result<i64>{
 let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;init(&tx)?;let(_,_,_,_,revision,_)=meta(&tx)?;let next=revision.checked_add(1).ok_or("chat revision exhausted")?;
 tx.execute("DELETE FROM chat_messages",[]).map_err(|e|e.to_string())?;
 tx.execute("DELETE FROM sqlite_sequence WHERE name='chat_messages'",[]).map_err(|e|e.to_string())?;
 tx.execute("UPDATE chat_meta SET summary='',summarized_seq=0,context_start_seq=0,scope='null',revision=?1 WHERE singleton=1",params![next]).map_err(|e|e.to_string())?;
 guard()?;tx.commit().map_err(|e|e.to_string())?;Ok(next)
}

/// Conversion between ordinary and encrypted workspaces preserves rows and revision.
pub fn copy_to(src:&Connection,dst:&Connection)->Result<()> {
 if table_exists(src,"chat_meta")? {
  if !table_exists(src,"chat_messages")?{return Err("invalid chat storage".into())}
  let(summary,summarized,start,scope,revision,_)=meta(src)?;schema(dst)?;
  dst.execute("INSERT INTO chat_meta VALUES(1,?1,?2,?3,?4,?5)",params![summary,summarized,start,scope.to_string(),revision]).map_err(|e|e.to_string())?;
  let mut st=src.prepare("SELECT seq,message FROM chat_messages ORDER BY seq").map_err(|e|e.to_string())?;let mut rows=st.query([]).map_err(|e|e.to_string())?;
  while let Some(row)=rows.next().map_err(|e|e.to_string())?{let seq:i64=row.get(0).map_err(|e|e.to_string())?;let raw:String=row.get(1).map_err(|e|e.to_string())?;let _=row_value(seq,raw.clone())?;dst.execute("INSERT INTO chat_messages(seq,message) VALUES(?1,?2)",params![seq,raw]).map_err(|e|e.to_string())?;}
 }else {
  let old=get(src)?;let revision=old["revision"].as_i64().ok_or("invalid chat revision")?;
  if revision>0{dst.execute_batch("CREATE TABLE chat_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1),state TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>=0))").map_err(|e|e.to_string())?;dst.execute("INSERT INTO chat_state VALUES(1,?1,?2)",params![old["state"].to_string(),revision]).map_err(|e|e.to_string())?;}
 }Ok(())
}
