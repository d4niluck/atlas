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
