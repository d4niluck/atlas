#[cfg(windows)]
fn main() -> Result<(),String> {
 let exe=std::env::var("ATLAS_REAL_SERVER").map_err(|e|e.to_string())?;let model=std::env::var("ATLAS_REAL_MODEL").map_err(|e|e.to_string())?;
 let now=std::time::Instant::now();let encoder=atlas_llama_probe::Encoder::start_device(&exe,&model,"cpu")?;
 println!("started {}s",now.elapsed().as_secs());
 use std::io::{Read,Write};for auth in ["", "Authorization: Bearer invalid\r\n", "Authorization: Bearer attacker-env-key\r\n", "Authorization: Bearer attacker-file-key\r\n"] {
  let mut socket=uds_windows::UnixStream::connect(encoder.socket_path()).map_err(|e|e.to_string())?;
  let body=r#"{"input":"test"}"#;
  write!(socket,"POST /v1/embeddings HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{}",body.len(),auth,body).map_err(|e|e.to_string())?;
  let mut reply=String::new();socket.read_to_string(&mut reply).map_err(|e|e.to_string())?;
  if !reply.starts_with("HTTP/1.0 401")&&!reply.starts_with("HTTP/1.1 401"){return Err(format!("unauthorized request was not rejected: {}",reply.lines().next().unwrap_or("")))}
 }
 println!("unauthorized, invalid and inherited attacker keys rejected");let output=encoder.encode("проверка поиска",true)?;
 if output.len()!=512{return Err("unexpected dimensions".into())}println!("embedding 512 dims; {}s",now.elapsed().as_secs());drop(encoder);println!("process unloaded");Ok(())
}

#[cfg(not(windows))] fn main() {}
