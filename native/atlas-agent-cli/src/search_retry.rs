use serde_json::Value;
use std::time::Duration;

/// Retry only the server's explicit index-not-ready response; do not retry authentication,
/// transport, or other failures. The initial attempt is followed by at most three retries.
pub fn search(mut request: impl FnMut() -> Result<(u16, Value), String>) -> Result<(u16, Value), String> {
    search_with_wait(&mut request, || std::thread::sleep(Duration::from_secs(1)))
}

fn search_with_wait(request: &mut impl FnMut() -> Result<(u16, Value), String>, mut wait: impl FnMut()) -> Result<(u16, Value), String> {
    for attempt in 0..=3 {
        let (status, mut value) = request()?;
        let pending = status == 503 && value["error"].as_str() == Some("Index not ready");
        if !pending { return Ok((status, value)); }
        if attempt == 3 {
            let progress=&value["progress"];let phase=progress["phase"].as_str().filter(|s|!s.is_empty());let completed=progress["completed"].as_u64();let total=progress["total"].as_u64();
            let detail=match (phase,completed,total){(Some(p),Some(c),Some(t))=>format!(" {p}: {c}/{t}."),(Some(p),_,_)=>format!(" {p}."),_=>String::new()};
            value["error"] = format!("Индекс пока не готов или обновляется.{detail} Попробуйте снова через 5 секунд; точное время окончания неизвестно").into();
            return Ok((status, value));
        }
        wait();
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;
    #[test]
    fn retries_only_index_pending_and_caps_attempts() {
        let calls=Cell::new(0); let waits=Cell::new(0);
        let mut pending=||{calls.set(calls.get()+1);Ok((503,json!({"error":"Index not ready"})))};
        let (status,value)=search_with_wait(&mut pending,||waits.set(waits.get()+1)).unwrap();
        assert_eq!((status,calls.get(),waits.get()),(503,4,3));assert!(value["error"].as_str().unwrap().contains("5 секунд"));
        let count=Cell::new(0);let mut denied=||{count.set(count.get()+1);Ok((403,json!({"error":"Forbidden"})))};
        assert_eq!(search_with_wait(&mut denied,||panic!("must not retry")).unwrap().0,403);assert_eq!(count.get(),1);
        let count=Cell::new(0);let mut ready=||{count.set(count.get()+1);if count.get()==1{Ok((503,json!({"error":"Index not ready"})))}else{Ok((200,json!({"results":[]})))}};
        assert_eq!(search_with_wait(&mut ready,||{}).unwrap().0,200);assert_eq!(count.get(),2);
    }
}
