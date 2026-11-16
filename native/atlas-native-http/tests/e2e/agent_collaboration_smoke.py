import json,os,pathlib,socket,subprocess,sys,tempfile,time,threading,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-edit-agent-') as t:
 root=pathlib.Path(t).resolve(); sockdir=root/'socket';sockdir.mkdir(mode=0o700)
 models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-model');(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}));mock=pathlib.Path(__file__).with_name('mock-portable-embedding-server.py').resolve()
 barrier=root/'barrier';barrier.mkdir();(barrier/'resume').touch()
 worker_barrier=root/'worker_barrier';worker_barrier.mkdir();(worker_barrier/'resume').touch()
 create_barrier=root/'create_barrier';create_barrier.mkdir();(create_barrier/'resume').touch()
 env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_AGENT_COMMIT_BARRIER=str(barrier),ATLAS_TEST_AGENT_WORKER_BARRIER=str(worker_barrier),ATLAS_TEST_AGENT_CREATE_WORKER_BARRIER=str(create_barrier),ATLAS_TEST_FAKE_ENCODER_LOG=str(root/'encoder.jsonl'),ATLAS_TEST_AGENT_FAIL_AFTER_DRAFT=str(root/'fail_draft'),ATLAS_TEST_AGENT_FAIL_AFTER_UPDATE=str(root/'fail_update'),ATLAS_TEST_FAIL_AGENT_ENQUEUE=str(root/'skip_enqueue'))
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket','--rag-model-dir',str(models),'--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None):
   raw=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=raw,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   try:
    with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  def agent(path,key,obj):
   data=json.dumps(obj).encode();s=socket.socket(socket.AF_UNIX);s.settimeout(10);s.connect(str(sockdir/'agent.sock'));s.sendall((f'POST {path} HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(data)}\r\n\r\n').encode()+data);buf=b''
   while True:
    part=s.recv(65536)
    if not part:break
    buf+=part
   s.close();head,body=buf.split(b'\r\n\r\n',1);return int(head.split()[1]),json.loads(body)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  profiles=[owner('/api/agents',{'name':n})[1] for n in ('a','b')]
  for prof in profiles:assert owner('/api/agents/'+prof['id'],{'name':prof['name'],'permissions':{'default':{'read':True,'edit':True,'add':True}}},'PUT')[0]==200
  create={'title':'Plain agent note','text':'plain content','request_id':'plain-create'}
  status,created=agent('/api/agent/default/notes',profiles[0]['token'],create);assert status==201,(status,created)
  status,replay=agent('/api/agent/default/notes',profiles[0]['token'],create);assert status==201 and replay['id']==created['id'] and replay['replayed'],(status,replay)
  assert agent('/api/agent/default/notes',profiles[0]['token'],{**create,'text':'changed'})[0]==409
  (root/'skip_enqueue').touch();status,local_failed=agent('/api/agent/default/notes',profiles[0]['token'],{'title':'Local queued','text':'local created searchable','request_id':'local-failure'});assert status==201,(status,local_failed)
  (root/'skip_enqueue').unlink();deadline=time.monotonic()+30;found_created=False
  while time.monotonic()<deadline:
   status,res=owner('/api/w/default/search',{'query':'local created searchable','mode':'semantic'})
   if status==200 and any(v.get('id')==local_failed['id'] for v in res.get('results',[])):found_created=True;break
   time.sleep(.1)
  assert found_created,('local create enqueue recovery',status,res)

  notes=[owner('/api/w/default/notes',{'title':n,'text':'old'})[1]['id'] for n in ('one','two')]
  path=lambda n:f'/api/agent/default/notes/{n}/edit'
  start=threading.Barrier(3);out=[]
  def contend(prof):
   start.wait();out.append((prof,agent(path(notes[0]),prof['token'],{'action':'acquire'})))
  threads=[threading.Thread(target=contend,args=(prof,)) for prof in profiles]
  for t in threads:t.start()
  start.wait()
  for t in threads:t.join(10);assert not t.is_alive()
  assert sorted(result[0] for _,result in out)==[200,409],out
  winner,result=next((prof,result) for prof,result in out if result[0]==200)
  assert agent(path(notes[0]),winner['token'],{'action':'release','lease':result[1]['lease']})[0]==200
  status,lease=agent(path(notes[0]),profiles[0]['token'],{'action':'acquire'});assert status==200,(status,lease)
  assert agent(path(notes[1]),profiles[0]['token'],{'action':'acquire'})[0]==409
  assert agent(path(notes[0]),profiles[1]['token'],{'action':'acquire'})[0]==409
  assert agent(path(notes[0]),profiles[0]['token'],{'action':'commit','lease':lease['lease'],'title':'edited','text':'new'})[0]==200
  assert owner('/api/w/default/notes/'+notes[0])[1]['text']=='new'
  deadline=time.monotonic()+15;found=False
  while time.monotonic()<deadline:
   status,res=owner('/api/w/default/search',{'query':'new','mode':'semantic'})
   if status==200 and any(x.get('id')==notes[0] and 'new' in str(x) for x in res.get('results',[])):found=True;break
   time.sleep(.1)
  assert found,('local baseline',status,res)
  (root/'skip_enqueue').touch()
  assert agent(path(notes[0]),profiles[0]['token'],{'action':'commit','lease':lease['lease'],'title':'edited','text':'durable_local_marker'})[0]==200
  (root/'skip_enqueue').unlink()
  deadline=time.monotonic()+8;found=False
  while time.monotonic()<deadline:
   status,res=owner('/api/w/default/search',{'query':'durable_local_marker','mode':'semantic'})
   if status==200 and any(x.get('id')==notes[0] and 'durable_local_marker' in str(x) for x in res.get('results',[])):found=True;break
   time.sleep(.1)
  assert found,('local failed to recover after skipped enqueue',status,res)

  # A human and another agent cannot take or delete the held note.
  assert owner('/api/w/default/notes/'+notes[0]+'/edit-lease',{'action':'acquire'})[0]==409
  assert agent('/api/agent/default/objects/'+notes[0]+'/delete',profiles[1]['token'],{})[0]==409
  assert agent(path(notes[0]),profiles[0]['token'],{'action':'release','lease':lease['lease']})[0]==200
  status,lease2=agent(path(notes[0]),profiles[1]['token'],{'action':'acquire'});assert status==200,(status,lease2)
  assert owner('/api/agents/'+profiles[1]['id'],{'name':'b','permissions':{}},'PUT')[0]==200
  assert agent(path(notes[0]),profiles[1]['token'],{'action':'commit','lease':lease2['lease'],'title':'forbidden','text':'forbidden'})[0] in (403,409)
  assert owner('/api/w/default/notes/'+notes[0])[1]['text']=='durable_local_marker'
  status,result=agent('/api/agent/default/objects/'+notes[0]+'/delete',profiles[0]['token'],{});assert status==200,(status,result)
  status,board=owner('/api/w/default/board');assert status==200 and not any(x['id']==notes[0] for x in board['objects']) and any(x['id']==notes[1] for x in board['objects'])
 finally:p.terminate();p.wait(timeout=10)
print('agent collaboration smoke passed')
