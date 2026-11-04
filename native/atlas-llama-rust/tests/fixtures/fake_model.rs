fn main() -> std::io::Result<()> {
    use std::io::{Read,Write};
    let args: Vec<String> = std::env::args().collect();
    let get=|flag:&str| args.iter().position(|v|v==flag).and_then(|i|args.get(i+1)).unwrap().clone();
    #[cfg(windows)] let key=std::env::var("LLAMA_API_KEY").expect("missing API key");
    #[cfg(unix)] let key=std::fs::read_to_string(get("--api-key-file"))?;
    let mut extras:Vec<String>=std::env::var("LLAMA_ARG_API_KEY_FILE").ok().and_then(|p|std::fs::read_to_string(p).ok()).into_iter().collect();
    #[cfg(unix)] if let Ok(v)=std::env::var("LLAMA_API_KEY"){extras.push(v)}
    if std::env::var_os("LLAMA_ARG_CONNECT").is_some(){return Err(std::io::Error::other("unsafe tunnel configuration"))}
    #[cfg(windows)] let listener=uds_windows::UnixListener::bind(get("--host"))?;
    #[cfg(unix)] let listener=std::os::unix::net::UnixListener::bind(get("--host"))?;
    for incoming in listener.incoming() {
        let mut stream=incoming?;
        let mut input=Vec::new(); let mut buf=[0u8;4096];
        loop {
            let n=stream.read(&mut buf)?; if n==0 {break} input.extend_from_slice(&buf[..n]);
            if let Some(end)=input.windows(4).position(|w|w==b"\r\n\r\n") {
                let header=String::from_utf8_lossy(&input[..end]);
                let size=header.lines().find_map(|line|line.strip_prefix("Content-Length: ")).and_then(|v|v.parse::<usize>().ok()).unwrap_or(0);
                if input.len()>=end+4+size {break}
            }
        }
        let header=String::from_utf8_lossy(&input);
        if header.starts_with("GET /health ") {if let Some(marker)=std::env::var_os("ATLAS_FAKE_STALL_HEALTH"){std::fs::write(marker,b"ready")?;std::thread::sleep(std::time::Duration::from_secs(30));}}
        if header.contains("DELAY") {std::thread::sleep(std::time::Duration::from_secs(5));}
        let authorised=header.lines().any(|line|line==format!("Authorization: Bearer {}",key.trim()) || extras.iter().any(|extra|line==format!("Authorization: Bearer {}",extra.trim())));
        if authorised && header.starts_with("POST /v1/chat/completions ") && header.contains("\"stream\":true") {
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {{\"choices\":[{{\"delta\":{{\"content\":\"answer \"}}}}]}}\n\n")?;
            stream.flush()?;
            if std::env::var_os("ATLAS_FAKE_STALL_STREAM").is_some(){std::thread::sleep(std::time::Duration::from_secs(30));continue}
            write!(stream,"data: {{\"choices\":[{{\"delta\":{{\"content\":\"[1]\"}}}}]}}\n\ndata: [DONE]\n\n")?;
            continue;
        }
        let (status,body)=if header.starts_with("GET /health ") {("200 OK",String::from("{}"))} else if !authorised {("401 Unauthorized",String::from("{}"))} else if header.starts_with("POST /tokenize ") {("200 OK",String::from("{\"tokens\":[1]}"))} else if header.starts_with("POST /v1/chat/completions ") {("200 OK",String::from("{\"choices\":[{\"message\":{\"content\":\"answer [1]\"}}]}"))} else if header.starts_with("POST /v1/embeddings ") {("200 OK",format!("{{\"data\":[{{\"embedding\":[{}]}}]}}",vec!["1";512].join(",")))} else {("404 Not Found",String::from("{}"))};
        write!(stream,"HTTP/1.0 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())?;
    }
    Ok(())
}
