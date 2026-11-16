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

  (barrier/'ready').unlink();(barrier/'resume').unlink(); commits=[]
  thread=threading.Thread(target=lambda:commits.append(agent(path(notes[0]),profiles[0]['token'],{'action':'commit','lease':lease['lease'],'title':'again','text':'delayed'})));thread.start()
  until=time.monotonic()+5
  while not (barrier/'ready').exists() and time.monotonic()<until:time.sleep(.01)
  assert (barrier/'ready').exists()
  assert owner('/api/agents/'+profiles[0]['id'],None,'DELETE')[0]==202
  assert owner('/api/agents/'+profiles[0]['id'],{'name':'a','permissions':{'default':{'read':True,'edit':True}}},'PUT')[0]==409
  (barrier/'resume').touch();thread.join(10);assert not thread.is_alive() and commits[0][0]==200
  until=time.monotonic()+5
  while time.monotonic()<until:
   status,_=owner('/api/agents/'+profiles[0]['id'],{'name':'a','permissions':{}},'PUT')
   if status==404:break
   assert status==409;time.sleep(.02)
  assert status==404

  status,last=agent(path(notes[0]),profiles[1]['token'],{'action':'acquire'});assert status==200
  assert agent(path(notes[0]),profiles[1]['token'],{'action':'release','lease':last['lease']})[0]==200
  for flag in ('fail_draft','fail_update'):
   status,held=agent(path(notes[0]),profiles[1]['token'],{'action':'acquire'});assert status==200,(status,held)
   (root/flag).touch()
   assert agent(path(notes[0]),profiles[1]['token'],{'action':'commit','lease':held['lease'],'title':'recover','text':flag})[0]==409
   assert agent(path(notes[0]),profiles[1]['token'],{'action':'release','lease':held['lease']})[0]==200
   (root/flag).unlink()
   status,recovered=owner('/api/w/default/note-drafts/recover',{});assert status==200,(status,recovered)
   assert owner('/api/w/default/notes/'+notes[0])[1]['text']==flag

  test_profile=owner('/api/agents',{'name':'storage'})[1];key=test_profile['token'];test_permissions={}
  status,locked=owner('/api/workspaces',{'name':'Protected','protected':True,'password':'test-password'});assert status==201,(status,locked)
  storage=[('protected',locked['id'])];external_paths={}
  disk=root/'disk';disk.mkdir()
  for is_protected in (False,True):
   cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})[1]['capability']
   args={'capability':cap,'name':'External '+str(is_protected),'protected':is_protected}
   if is_protected:args['password']='test-password'
   status,made=owner('/api/workspaces/create-external',args);assert status==201,(status,made)
   args={'capability':made['capability']}
   if is_protected:args.update({'password':'test-password','test_protected':True})
   status,attached=owner('/api/workspaces/attach',args);assert status==200,(is_protected,status,attached,made)
   external_paths[attached['id']]=made['path'];storage.append(('external protected' if is_protected else 'external',attached['id']))
  for kind,wid in storage:
   status,note=owner('/api/w/'+wid+'/notes',{'title':kind,'text':'old'});assert status==201,(kind,status,note)
   test_permissions[wid]={'read':True,'edit':True,'add':True}
   assert owner('/api/agents/'+test_profile['id'],{'name':'storage','permissions':test_permissions},'PUT')[0]==200
   create_path=f'/api/agent/{wid}/notes'
   request={'title':'Created by agent','text':'agent content','request_id':'same-request'}
   status,created=agent(create_path,key,request);assert status==201,(kind,status,created)
   status,replay=agent(create_path,key,request);assert status==201 and replay['id']==created['id'] and replay['replayed'],(kind,status,replay)
   status,_=agent(create_path,key,{**request,'text':'different'});assert status==409,(kind,status)
   status,board=owner('/api/w/'+wid+'/board');assert status==200 and len([v for v in board['objects'] if v['id']==created['id']])==1,(kind,board)
   (root/'skip_enqueue').touch()
   status,failed_enqueue=agent(create_path,key,{'title':'Queued','text':'new agent create indexed','request_id':'failed-enqueue'});assert status==201,(kind,status,failed_enqueue)
   (root/'skip_enqueue').unlink();deadline=time.monotonic()+30;found_created=False
   while time.monotonic()<deadline:
    status,res=owner('/api/w/'+wid+'/search',{'query':'new agent create indexed','mode':'semantic'})
    if status==200 and any(v.get('id')==failed_enqueue['id'] for v in res.get('results',[])):found_created=True;break
    time.sleep(.1)
   assert found_created,(kind,'create enqueue recovery',status,res)

   ap=f'/api/agent/{wid}/notes/{note["id"]}/edit'
   status,held=agent(ap,key,{'action':'acquire'});assert status==200,(kind,status,held)
   status,result=agent(ap,key,{'action':'commit','lease':held['lease'],'title':kind,'text':'fresh'});assert status==200,(kind,status,result)
   assert owner('/api/w/'+wid+'/notes/'+note['id'])[1]['text']=='fresh',kind
   deadline=time.monotonic()+40;found=False
   while time.monotonic()<deadline:
    status,res=owner('/api/w/'+wid+'/search',{'query':'fresh','mode':'semantic'})
    if status==200 and any(x.get('id')==note['id'] and 'fresh' in str(x) for x in res.get('results',[])):found=True;break
    time.sleep(.1)
   assert found,(kind,status,res)
   (root/'skip_enqueue').touch()
   status,result=agent(ap,key,{'action':'commit','lease':held['lease'],'title':kind,'text':'durable_changed_marker'});assert status==200,(kind,status,result)
   (root/'skip_enqueue').unlink()
   deadline=time.monotonic()+8;found=False
   while time.monotonic()<deadline:
    status,res=owner('/api/w/'+wid+'/search',{'query':'durable_changed_marker','mode':'semantic'})
    if status==200 and any(x.get('id')==note['id'] and 'durable_changed_marker' in str(x) for x in res.get('results',[])):found=True;break
    time.sleep(.1)
   assert found,(kind,'failed to recover after skipped enqueue',status,res)

   assert agent(ap,key,{'action':'release','lease':held['lease']})[0]==200
   if kind in ('protected','external','external protected'):
    test_permissions[wid].update(semantic=True,lexical=True)
    assert owner('/api/agents/'+test_profile['id'],{'name':'storage','permissions':test_permissions},'PUT')[0]==200
    for mode in ('semantic','hybrid'):
     deadline=time.monotonic()+20
     while time.monotonic()<deadline:
      status,result=agent(f'/api/agent/{wid}/search',key,{'query':'durable_changed_marker','mode':mode})
      if status==200:break
      assert status==503,(kind,mode,status,result);time.sleep(.1)
     assert status==200 and any(x.get('id')==note['id'] for x in result.get('results',[])),(kind,mode,status,result)
   if kind=='external':
    (create_barrier/'ready').unlink(missing_ok=True);(create_barrier/'resume').unlink();slow_create=[];closed=[]
    create_thread=threading.Thread(target=lambda:slow_create.append(agent(create_path,key,{'title':'close race','text':'close race','request_id':'close-race'})));create_thread.start();deadline=time.monotonic()+5
    while not (create_barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
    assert (create_barrier/'ready').exists();close_thread=threading.Thread(target=lambda:closed.append(owner('/api/workspaces/'+wid+'/close',{})));close_thread.start();time.sleep(.2);assert close_thread.is_alive(),'close must wait for create'
    (create_barrier/'resume').touch();create_thread.join(10);close_thread.join(10);assert not create_thread.is_alive() and slow_create[0][0]==201 and closed[0][0]==200,(slow_create,closed)
    status,v=owner('/api/workspaces/authorize',{'operation':'attach','path':external_paths[wid]});assert status==200,(status,v);status,v=owner('/api/workspaces/attach',{'capability':v['capability'],'id':wid});assert status==200,(status,v)
    status,replayed=agent(create_path,key,{'title':'close race','text':'close race','request_id':'close-race'});assert status==201 and replayed['replayed'],(status,replayed)
    slow=owner('/api/agents',{'name':'slow-create'})[1];assert owner('/api/agents/'+slow['id'],{'name':'slow-create','permissions':{wid:{'add':True}}},'PUT')[0]==200
    (create_barrier/'ready').unlink(missing_ok=True);(create_barrier/'resume').unlink();pending_create=[]
    thread=threading.Thread(target=lambda:pending_create.append(agent(create_path,slow['token'],{'title':'slow','text':'slow','request_id':'slow-create'})));thread.start();deadline=time.monotonic()+5
    while not (create_barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
    assert (create_barrier/'ready').exists();status,_=owner('/api/agents/'+slow['id'],None,'DELETE');assert status==202,status
    status,catalog=owner('/api/agents');assert status==200 and slow['id'] in catalog['pending_ids'],catalog
    (create_barrier/'resume').touch();thread.join(10);assert not thread.is_alive() and pending_create[0][0]==201,pending_create
    assert agent(create_path,slow['token'],{'title':'slow','text':'slow','request_id':'slow-create'})[0]==403
    (worker_barrier/'ready').unlink(missing_ok=True);(worker_barrier/'resume').unlink();acquired=[]
    thread=threading.Thread(target=lambda:acquired.append(agent(ap,key,{'action':'acquire'})));thread.start();deadline=time.monotonic()+5
    while not (worker_barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
    assert (worker_barrier/'ready').exists()
    assert owner('/api/agents/'+test_profile['id'],{'name':'storage','permissions':{}},'PUT')[0]==200
    (worker_barrier/'resume').touch();thread.join(10);assert not thread.is_alive() and acquired[0][0]==409,acquired
    assert owner('/api/agents/'+test_profile['id'],{'name':'storage','permissions':test_permissions},'PUT')[0]==200

  test_permissions['default']={'read':True,'semantic':True,'lexical':False}
  assert owner('/api/agents/'+test_profile['id'],{'name':'storage','permissions':test_permissions},'PUT')[0]==200
  search='/api/agent/default/search';status,_=agent(search,key,{'query':'old','mode':'hybrid'});assert status==403,status
  deadline=time.monotonic()+20
  while time.monotonic()<deadline:
   status,result=agent(search,key,{'query':'old','mode':'semantic'})
   if status==200:break
   assert status==503,(status,result);time.sleep(.1)
  assert status==200,(status,result)
  test_permissions['default']['lexical']=True
  assert owner('/api/agents/'+test_profile['id'],{'name':'storage','permissions':test_permissions},'PUT')[0]==200
  status,result=agent(search,key,{'query':'old','mode':'hybrid'});assert status==200,(status,result)
 finally:p.terminate();p.wait(timeout=10)
print('agent edit lease passed')
