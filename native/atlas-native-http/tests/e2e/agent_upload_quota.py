import hashlib,json,os,pathlib,socket,subprocess,sys,tempfile,threading,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-upload-e2e-',dir='/private/tmp') as t:
 local = len(sys.argv)>2 and sys.argv[2] in ('local-protected','local-plain'); protected = len(sys.argv)>2 and sys.argv[2] in ('protected','local-protected'); root=pathlib.Path(t); sockdir=root/'sock';sockdir.mkdir(mode=0o700); disk=root/'disk';disk.mkdir()
 tmp=root/'tmp';tmp.mkdir();barrier=root/'finish-barrier';barrier.mkdir();commit_barrier=root/'commit-barrier';commit_barrier.mkdir();chunk_barrier=root/'chunk-barrier';chunk_barrier.mkdir();env=dict(os.environ,TMPDIR=str(tmp),ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_AGENT_UPLOAD='1',ATLAS_TEST_UPLOAD_IDLE_MS='60000',ATLAS_TEST_AGENT_UPLOAD_FINISH_BARRIER=str(barrier),ATLAS_TEST_AGENT_UPLOAD_COMMIT_BARRIER=str(commit_barrier),ATLAS_TEST_AGENT_UPLOAD_CHUNK_BARRIER=str(chunk_barrier))
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env,start_new_session=True)
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit(),port
  def owner(path,data=None,method=None):
   b=None if data is None else json.dumps(data).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=b,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   try:
    with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  def agent(wid,key,obj):
   data=json.dumps(obj).encode();s=socket.socket(socket.AF_UNIX);s.settimeout(10);s.connect(str(sockdir/'agent.sock'));s.sendall((f'POST /api/agent/{wid}/uploads HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n\r\n').encode()+data);out=b''
   while True:
    b=s.recv(65536)
    if not b:break
    out+=b
   s.close();head,body=out.split(b'\r\n\r\n',1);return int(head.split()[1]),json.loads(body)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  status,w=owner('/api/workspaces',{'name':'Quota','protected':True,'password':'test-password'});assert status==201,(status,w);wid=w['id'];profiles=[]
  for name in ('quota-a','quota-b'):
   a=owner('/api/agents',{'name':name})[1];assert owner('/api/agents/'+a['id'],{'name':name,'permissions':{wid:{'add':True}}},'PUT')[0]==200;profiles.append(a)
  a,b=profiles;large={'action':'begin','name':'large.bin','size':10*1024*1024*1024,'request_id':'exact-max','digest':'0'*64}
  status,r=agent(wid,a['token'],large);assert status==201,(status,r);upload=r['upload']
  small={'action':'begin','name':'small.bin','size':1,'request_id':'small','digest':'0'*64}
  status,r=agent(wid,b['token'],small);assert status==429 and r.get('code')=='upload_quota',(status,r)
  assert agent(wid,a['token'],{'action':'cancel','upload':upload})[0]==202
  deadline=time.monotonic()+5
  while time.monotonic()<deadline:
   status,r=agent(wid,b['token'],small)
   if status==201:break
   time.sleep(.05)
  assert status==201,(status,r);assert agent(wid,b['token'],{'action':'cancel','upload':r['upload']})[0]==202
  print('agent upload quota passed (no payload bytes written)')
 finally:
  import signal
  try:os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  p.wait(timeout=10)
