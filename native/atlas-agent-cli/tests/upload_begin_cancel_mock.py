import json,os,pathlib,signal,socket,subprocess,sys,tempfile,threading,time
with tempfile.TemporaryDirectory(prefix='atlas-beg-',dir='/tmp') as t:
 root=pathlib.Path(t);directory=root/'socket';directory.mkdir(mode=0o700);path=directory/'agent.sock';server=socket.socket(socket.AF_UNIX);server.bind(str(path));path.chmod(0o600);server.listen();server.settimeout(.2);key=root/'key';key.write_text('key');key.chmod(0o600);source=root/'file';source.write_bytes(b'abc');begin=threading.Event();cancel=threading.Event();stop=threading.Event();release=threading.Event()
 def handle(c):
  with c:
   data=b''
   while b'\r\n\r\n' not in data:data+=c.recv(4096)
   h,b=data.split(b'\r\n\r\n',1);n=int(next(x.split(b':',1)[1] for x in h.split(b'\r\n') if x.lower().startswith(b'content-length:')))
   while len(b)<n:b+=c.recv(4096)
   v=json.loads(b)
   if v['action']=='begin':begin.set();release.wait(8);return
   assert v=={'action':'cancel','request_id':'req'};cancel.set();release.set();raw=b'{"ok":true}';c.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n'+raw)
 def serve():
  while not stop.is_set():
   try:c,_=server.accept()
   except socket.timeout:continue
   threading.Thread(target=handle,args=(c,),daemon=True).start()
 thread=threading.Thread(target=serve);thread.start();env=os.environ.copy();env['ATLAS_TEST_AGENT_SOCKET_DIR']=str(directory)
 try:
  proc=subprocess.Popen([sys.argv[1],'--key-file',str(key),'upload','default',str(source),'req'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  assert begin.wait(5);start=time.monotonic();proc.send_signal(signal.SIGINT);out,err=proc.communicate(timeout=5);assert time.monotonic()-start<3 and cancel.is_set() and proc.returncode!=0,(out,err)
 finally:release.set();stop.set();thread.join(timeout=5);server.close()
print('hanging begin cancellation sent by request ID')
