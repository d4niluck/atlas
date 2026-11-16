import json, os, pathlib, socket, subprocess, sys, tempfile, threading

# Exercise the actual executable and capture the request sent to a private fixture.
with tempfile.TemporaryDirectory(prefix='atlas-pages-',dir='/tmp') as temp:
 root=pathlib.Path(temp); sockdir=root/'socket';sockdir.mkdir(mode=0o700)
 path=sockdir/'agent.sock';server=socket.socket(socket.AF_UNIX);server.bind(str(path));path.chmod(0o600);server.listen();server.settimeout(5)
 key=root/'key';key.write_text('test-key');key.chmod(0o600);requests=[]
 def serve():
  try:
   for _ in range(2):
    conn,_=server.accept()
    with conn:
     data=b''
     while b'\r\n\r\n' not in data:data+=conn.recv(4096)
     head,body=data.split(b'\r\n\r\n',1);size=int(next(x.split(b':',1)[1] for x in head.split(b'\r\n') if x.lower().startswith(b'content-length:')))
     while len(body)<size:body+=conn.recv(4096)
     requests.append(json.loads(body));payload=json.dumps({'results':[{'id':'x','title':'Source','chunks':[{'text':'Text A'},{'text':'Text B'}]}],'total':30,'offset':0,'limit':10}).encode()
     conn.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: '+str(len(payload)).encode()+b'\r\nConnection: close\r\n\r\n'+payload)
  except socket.timeout:pass
 worker=threading.Thread(target=serve,daemon=True);worker.start();env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir))
 default=subprocess.run([sys.argv[1],'--key-file',str(key),'search','default','query'],env=env,capture_output=True,text=True,timeout=10)
 page=subprocess.run([sys.argv[1],'--key-file',str(key),'--offset','10','--limit','10','--json','search','default','query','semantic'],env=env,capture_output=True,text=True,timeout=10)
 server.close();worker.join(timeout=6)
 assert default.returncode==0,default.stderr
 assert '# Chunk 1\nSource  [x]\nText A\n\n# Chunk 2\nSource  [x]\nText B' in default.stdout,default.stdout
 assert page.returncode==0,page.stderr
 assert requests[1]['offset']==10 and requests[1]['limit']==10 and requests[1]['mode']=='semantic',requests
print('search pages and chunk formatting contract passed')
