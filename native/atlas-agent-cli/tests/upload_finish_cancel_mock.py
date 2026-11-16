import json,os,pathlib,signal,socket,subprocess,sys,tempfile,threading,time
with tempfile.TemporaryDirectory(prefix='atlas-fin-',dir='/tmp') as t:
 root=pathlib.Path(t);directory=root/'socket';directory.mkdir(mode=0o700);path=directory/'agent.sock';server=socket.socket(socket.AF_UNIX);server.bind(str(path));path.chmod(0o600);server.listen();server.settimeout(.2);key=root/'key';key.write_text('key');key.chmod(0o600);source=root/'file';source.write_bytes(b'abc');started=threading.Event();cancel=threading.Event();stop=threading.Event();release=threading.Event()
 def handle(c):
  with c:
   data=b''
   while b'\r\n\r\n' not in data:data+=c.recv(4096)
   h,b=data.split(b'\r\n\r\n',1);n=int(next(x.split(b':',1)[1] for x in h.split(b'\r\n') if x.lower().startswith(b'content-length:')))
   while len(b)<n:b+=c.recv(4096)
   v=json.loads(b)
   if v['action']=='finish':started.set();release.wait(8);return
   if v['action']=='cancel':cancel.set();release.set();reply={'ok':True,'pending':True};status='202 Accepted'
   else:reply={'upload':'u'} if v['action']=='begin' else {'offset':3};status='200 OK'
   raw=json.dumps(reply).encode();c.sendall(f'HTTP/1.1 {status}\r\nContent-Length: {len(raw)}\r\nConnection: close\r\n\r\n'.encode()+raw)
 def serve():
  while not stop.is_set():
   try:c,_=server.accept()
   except socket.timeout:continue
   threading.Thread(target=handle,args=(c,),daemon=True).start()
 thread=threading.Thread(target=serve);thread.start();env=os.environ.copy();env['ATLAS_TEST_AGENT_SOCKET_DIR']=str(directory)
 try:
  proc=subprocess.Popen([sys.argv[1],'--key-file',str(key),'upload','default',str(source),'req'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  assert started.wait(5);start=time.monotonic();proc.send_signal(signal.SIGINT);out,err=proc.communicate(timeout=5);assert time.monotonic()-start<3 and cancel.is_set() and proc.returncode!=0 and 'сохранение могло завершиться' in err,(out,err)
 finally:release.set();stop.set();thread.join(timeout=5);server.close()
print('uncertain finish cancellation reported')
