import hashlib,json,os,pathlib,socket,subprocess,sys,tempfile,threading
with tempfile.TemporaryDirectory(prefix='atlas-upload-',dir='/tmp') as t:
 root=pathlib.Path(t);directory=root/'socket';directory.mkdir(mode=0o700);socket_path=directory/'agent.sock';server=socket.socket(socket.AF_UNIX);server.bind(str(socket_path));socket_path.chmod(0o600);server.listen();key=root/'key';key.write_text('test-key');key.chmod(0o600)
 source=root/'data.bin';content=bytes(range(256))*200;source.write_bytes(content);received=bytearray();actions=[];errors=[]
 def serve():
  try:
   while True:
    conn,_=server.accept()
    with conn:
     data=b''
     while b'\r\n\r\n' not in data:data+=conn.recv(4096)
     head,body=data.split(b'\r\n\r\n',1);n=int(next(v.split(b':',1)[1] for v in head.split(b'\r\n') if v.lower().startswith(b'content-length:')))
     while len(body)<n:body+=conn.recv(65536)
     v=json.loads(body);actions.append(v['action'])
     if v['action']=='begin':
      assert v['size']==len(content) and v['digest']==hashlib.sha256(content).hexdigest() and v['request_id']=='req-1';reply={'upload':'up1'}
     elif v['action']=='chunk':
      assert v['offset']==len(received) and len(v['bytes'])<=16000;received.extend(v['bytes']);reply={'offset':len(received)}
     elif v['action']=='finish':
      assert bytes(received)==content;reply={'id':'abc','replayed':False}
     else:raise AssertionError(v)
     raw=json.dumps(reply).encode();conn.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: '+str(len(raw)).encode()+b'\r\nConnection: close\r\n\r\n'+raw)
     if v['action']=='finish':break
  except Exception as exc:errors.append(exc)
 worker=threading.Thread(target=serve,daemon=True);worker.start();env=os.environ.copy();env['ATLAS_TEST_AGENT_SOCKET_DIR']=str(directory)
 try:
  result=subprocess.run([sys.argv[1],'--key-file',str(key),'--json','upload','default',str(source),'req-1'],env=env,text=True,capture_output=True,timeout=15)
  assert result.returncode==0,(result.stderr,errors);assert json.loads(result.stdout)['id']=='abc'
 finally:worker.join(timeout=5);server.close()
 assert not worker.is_alive() and not errors,errors;assert actions==['begin','chunk','chunk','chunk','chunk','finish'],actions
print('chunked upload CLI contract passed')
