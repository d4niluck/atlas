import signal,json,os,pathlib,socket,subprocess,sys,tempfile,threading,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-pending-') as tmp:
 root=pathlib.Path(tmp).resolve();sockdir=root/'socket';sockdir.mkdir(mode=0o700);barrier=root/'barrier';barrier.mkdir();dest=root/'exports';dest.mkdir();p=None
 env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_AGENT_CREATE_BARRIER=str(barrier),ATLAS_TEST_AGENT_CREATE_PHASE='published')
 def start():
  global p,port
  p=subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()),'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,env=env,start_new_session=True);p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  for _ in range(200):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
 def owner(path,value=None,method=None):
  b=None if value is None else json.dumps(value).encode();r=urllib.request.Request('http://127.0.0.1:'+port+path,data=b,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
  try:
   with urllib.request.urlopen(r,timeout=10) as v:return v.status,json.load(v)
  except urllib.error.HTTPError as e:return e.code,json.load(e)
 def agent(key,wid):
  b=json.dumps({'title':'Partial','text':'preserve me','request_id':'req'}).encode();s=socket.socket(socket.AF_UNIX);s.settimeout(15);s.connect(str(sockdir/'agent.sock'));s.sendall((f'POST /api/agent/{wid}/notes HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(b)}\r\n\r\n').encode()+b)
  try:
   while s.recv(65536):pass
  except (OSError,TimeoutError):pass
  s.close()
 try:
  start();kind=sys.argv[2] if len(sys.argv)>2 else 'local';assert kind in ('local','external');wid='default';source=root/'data/.default'
  if kind=='external':
   disk=root/'disk';disk.mkdir();cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})[1]['capability'];code,made=owner('/api/workspaces/create-external',{'capability':cap,'name':'External','protected':False});assert code==201,(code,made);source=pathlib.Path(made['path']);code,v=owner('/api/workspaces/attach',{'capability':made['capability']});assert code==200,(code,v);wid=v['id']
  profile=owner('/api/agents',{'name':'a'})[1];assert owner('/api/agents/'+profile['id'],{'name':'a','permissions':{wid:{'add':True}}},'PUT')[0]==200
  t=threading.Thread(target=agent,args=(profile['token'],wid));t.start();deadline=time.monotonic()+10
  while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.02)
  assert (barrier/'ready').exists();os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=10);t.join(5);assert not t.is_alive();env.pop('ATLAS_TEST_AGENT_CREATE_BARRIER');env.pop('ATLAS_TEST_AGENT_CREATE_PHASE');start()
  if kind=='external':
   deadline=time.monotonic()+15
   while True:
    code,capability=owner('/api/workspaces/authorize',{'operation':'attach','path':str(source)})
    if code==200:break
    assert time.monotonic()<deadline,(code,capability);time.sleep(.1)
   code,value=owner('/api/workspaces/attach',{'capability':capability['capability'],'id':wid});assert code==200,(code,value)
  code,value=owner(f'/api/w/{wid}/agent-create-pending');assert code==200 and len(value['pending'])==1,(code,value);pending=value['pending'][0]
  def cap():return owner('/api/workspaces/authorize',{'operation':'export','path':str(dest)})[1]['capability']
  for move in (False,True):
   code,value=owner('/api/workspaces/export',{'capability':cap(),'ids':[wid],'move':move});assert code>=400,(move,code,value);assert (source/'atlas.sqlite3').is_file()
  code,value=owner(f'/api/w/{wid}/agent-create-cancel',{'actor':pending['actor'],'request_id':pending['request_id']});assert code==200 and value['cancelled'],(code,value);recovered=source/value['saved_as'];assert recovered.read_text()=='preserve me'
  status,catalog=owner(f'/api/w/{wid}/agent-create-recovered');assert status==200 and len(catalog['recovered'])==1,(status,catalog)
  status,content=owner(f'/api/w/{wid}/agent-create-recovered/read',{'actor':pending['actor'],'request_id':pending['request_id']});assert status==200 and content['text']=='preserve me',(status,content)
  status,restored=owner(f'/api/w/{wid}/agent-create-recovered/restore',{'actor':pending['actor'],'request_id':pending['request_id']});assert status==200 and restored['restored'],(status,restored)
  status,note=owner(f'/api/w/{wid}/notes/'+restored['id']);assert status==200 and note['text']=='preserve me',(status,note)
  status,catalog=owner(f'/api/w/{wid}/agent-create-recovered');assert status==200 and not catalog['recovered'],catalog
  code,value=owner('/api/workspaces/export',{'capability':cap(),'ids':[wid],'move':False});assert code==200,(code,value);assert not recovered.exists();assert not list(recovered.parent.glob('.atlas-agent-finalized-*'))
  if kind=='local':
   status,value=owner(f'/api/workspaces/{wid}/close',{});assert status==200,(status,value)
   status,value=owner(f'/api/workspaces/{wid}/protection',{'protected':True,'password':'test-password'},'PATCH');assert status==200,(kind,status,value)
   assert (source/'workspace.atlas').is_file()
   for path in source.rglob('*'):
    if path.is_file():assert b'preserve me' not in path.read_bytes(),path
   assert not list(source.parent.glob('.atlas-unprotected-*'))
   status,value=owner(f'/api/workspaces/{wid}/unlock',{'password':'test-password'});assert status==200,(status,value)
   status,note=owner(f'/api/w/{wid}/notes/'+restored['id']);assert status==200 and note['text']=='preserve me',(status,note)
 finally:
  if p and p.poll() is None:p.terminate();p.wait(timeout=10)
print('agent external pending recovery passed')
