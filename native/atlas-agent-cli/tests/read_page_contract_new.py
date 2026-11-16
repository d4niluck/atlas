import json,os,pathlib,socket,subprocess,sys,tempfile,threading,urllib.parse
with tempfile.TemporaryDirectory(prefix='atlas-read-page-',dir='/tmp') as t:
 root=pathlib.Path(t);d=root/'sock';d.mkdir(mode=0o700);path=d/'agent.sock';s=socket.socket(socket.AF_UNIX);s.bind(str(path));path.chmod(0o600);s.listen();s.settimeout(5)
 key=root/'key';key.write_text('fixture');key.chmod(0o600);env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(d));seen=[]
 def serve():
  try:
   for _ in range(2):
    c,_=s.accept()
    with c:
     data=b''
     while b'\r\n\r\n' not in data:data+=c.recv(4096)
     url=data.split(b'\r\n',1)[0].decode().split(' ')[1];seen.append(url);q=urllib.parse.parse_qs(urllib.parse.urlsplit(url).query);offset=int(q.get('offset',['0'])[0]);payload=json.dumps({'id':'abc','title':'Doc.pdf','text':'фрагмент','offset':offset,'next_offset':4000 if offset==0 else None,'total':5000}).encode();c.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: '+str(len(payload)).encode()+b'\r\nConnection: close\r\n\r\n'+payload)
  except (socket.timeout,OSError):pass
 th=threading.Thread(target=serve);th.start()
 first=subprocess.run([sys.argv[1],'--key-file',str(key),'read','default','abc'],env=env,text=True,capture_output=True,timeout=8)
 second=subprocess.run([sys.argv[1],'--key-file',str(key),'--offset','4000','--limit','1000','read','default','abc'],env=env,text=True,capture_output=True,timeout=8)
 th.join(timeout=6);s.close()
 assert first.returncode==0 and 'фрагмент' in first.stdout,(first.stdout,first.stderr)
 assert '4000' in first.stdout and 'read' in first.stdout,first.stdout
 assert second.returncode==0,second.stderr
 assert len(seen)==2 and urllib.parse.parse_qs(urllib.parse.urlsplit(seen[1]).query)=={'offset':['4000'],'limit':['1000']},seen
print('read paging contract passed')
