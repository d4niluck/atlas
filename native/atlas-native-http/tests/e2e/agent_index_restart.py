import json,os,pathlib,shutil,socket,subprocess,sys,tempfile,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-index-restart-') as t:
 root=pathlib.Path(t).resolve();sockdir=root/'socket';sockdir.mkdir(mode=0o700);models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake')
 settings={'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':False,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'};(models/'settings.json').write_text(json.dumps(settings))
 env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_FAKE_ENCODER_LOG=str(root/'encoder.jsonl'))
 mock=pathlib.Path(__file__).with_name('mock-portable-embedding-server.py').resolve();p=None
 def start():
  global p,port
  p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket','--rag-model-dir',str(models),'--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,env=env);p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  for _ in range(200):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
 def owner(path,value=None,method=None):
  b=None if value is None else json.dumps(value).encode();r=urllib.request.Request('http://127.0.0.1:'+port+path,data=b,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
  try:
   with urllib.request.urlopen(r,timeout=10) as v:return v.status,json.load(v)
  except urllib.error.HTTPError as e:return e.code,json.load(e)
 def agent(path,key,value):
  b=json.dumps(value).encode();s=socket.socket(socket.AF_UNIX);s.settimeout(10);s.connect(str(sockdir/'agent.sock'));s.sendall((f'POST {path} HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(b)}\r\n\r\n').encode()+b);out=b''
  while True:
   part=s.recv(65536)
   if not part:break
   out+=part
  s.close();head,body=out.split(b'\r\n\r\n',1);return int(head.split()[1]),json.loads(body)
 try:
  kind=sys.argv[2] if len(sys.argv)>2 else 'local';assert kind in ('local','protected','external','external-protected')
  start();wid='default';external_path=None
  if kind=='protected':
   status,v=owner('/api/workspaces',{'name':'protected','protected':True,'password':'password'});assert status==201,(status,v);wid=v['id']
  if kind.startswith('external'):
   disk=root/'disk';disk.mkdir();cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})[1]['capability'];protected=kind=='external-protected'
   body={'capability':cap,'name':'external','protected':protected}
   if protected:body['password']='password'
   status,made=owner('/api/workspaces/create-external',body);assert status==201,(status,made);external_path=made['path'];body={'capability':made['capability']}
   if protected:body.update(password='password',test_protected=True)
   status,v=owner('/api/workspaces/attach',body);assert status==200,(status,v);wid=v['id']
  profile=owner('/api/agents',{'name':'a'})[1];owner('/api/agents/'+profile['id'],{'name':'a','permissions':{wid:{'read':True,'edit':True}}},'PUT');status,v=owner('/api/w/'+wid+'/notes',{'title':'A','text':'old'});assert status==201,(status,v);note=v['id'];path=f'/api/agent/{wid}/notes/{note}/edit';status,held=agent(path,profile['token'],{'action':'acquire'});assert status==200,(kind,status,held);assert agent(path,profile['token'],{'action':'commit','lease':held['lease'],'title':'A','text':'restart marker value'})[0]==200
  if kind=='local':assert (root/'data/.default/.atlas-agent-index-dirty').exists()
  p.kill();p.wait(timeout=10)
  if external_path:
   copied=root/'portable-copy';shutil.copytree(external_path,copied);external_path=str(copied)
  settings['semantic_enabled']=True;(models/'settings.json').write_text(json.dumps(settings));start()
  if kind=='protected':
   status,v=owner('/api/workspaces/'+wid+'/unlock',{'password':'password'});assert status==200,(status,v)
  if kind.startswith('external'):
   status,v=owner('/api/workspaces/authorize',{'operation':'attach','path':external_path});assert status==200,(status,v)
   body={'capability':v['capability'],'id':wid}
   if kind=='external-protected':body.update(password='password',test_protected=True)
   status,v=owner('/api/workspaces/attach',body);assert status==200,(status,v)
  deadline=time.monotonic()+30;found=False
  while time.monotonic()<deadline:
   status,res=owner('/api/w/'+wid+'/search',{'query':'restart marker value','mode':'semantic'})
   if status==200 and any(x.get('id')==note and 'restart marker value' in str(x) for x in res.get('results',[])):found=True;break
   time.sleep(.1)
  assert found,(kind,status,res)
  if kind=='local':assert not (root/'data/.default/.atlas-agent-index-dirty').exists()

 finally:
  if p and p.poll() is None:p.terminate();p.wait(timeout=10)
print('agent index restart passed')
