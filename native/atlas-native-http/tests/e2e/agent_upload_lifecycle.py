import hashlib,json,os,pathlib,socket,subprocess,sys,tempfile,threading,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-upload-e2e-',dir='/private/tmp') as t:
 local = len(sys.argv)>2 and sys.argv[2] in ('local-protected','local-plain'); protected = len(sys.argv)>2 and sys.argv[2] in ('protected','local-protected'); root=pathlib.Path(t); sockdir=root/'sock';sockdir.mkdir(mode=0o700); disk=root/'disk';disk.mkdir()
 tmp=root/'tmp';tmp.mkdir();barrier=root/'finish-barrier';barrier.mkdir();commit_barrier=root/'commit-barrier';commit_barrier.mkdir();chunk_barrier=root/'chunk-barrier';chunk_barrier.mkdir();env=dict(os.environ,TMPDIR=str(tmp),ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_AGENT_UPLOAD='1',ATLAS_TEST_UPLOAD_IDLE_MS='2000',ATLAS_TEST_AGENT_UPLOAD_FINISH_BARRIER=str(barrier),ATLAS_TEST_AGENT_UPLOAD_COMMIT_BARRIER=str(commit_barrier),ATLAS_TEST_AGENT_UPLOAD_CHUNK_BARRIER=str(chunk_barrier))
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
  if local:
   args={'name':'Local','protected':protected};args.update({'password':'test-password'} if protected else {});status,w=owner('/api/workspaces',args);assert status==201,(status,w);wid=w['id']
  else:
   cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})[1]['capability'];create={'capability':cap,'name':'Upload','protected':protected};create.update({'password':'test-password'} if protected else {});made=owner('/api/workspaces/create-external',create)[1];attach={'capability':made['capability']};attach.update({'password':'test-password','test_protected':True} if protected else {});status,w=owner('/api/workspaces/attach',attach);assert status==200,(status,w);wid=w['id']
  a=owner('/api/agents',{'name':'a'})[1];b=owner('/api/agents',{'name':'b'})[1];key=a['token'];assert agent(wid,key,{'action':'begin','name':'file.txt','size':3})[0]==403
  for item in (a,b):assert owner('/api/agents/'+item['id'],{'name':item['name'],'permissions':{wid:{'read':True,'add':True}}},'PUT')[0]==200
  def transient(name,size):
   value={'action':'begin','name':name,'size':size}
   if local:value.update({'request_id':str(time.monotonic_ns()),'digest':hashlib.sha256(b'').hexdigest()})
   return value
  if True:
   data=b'hello from agent'; digest=hashlib.sha256(data).hexdigest();begin={'action':'begin','name':'hello.txt','size':len(data),'request_id':'file-one','digest':digest}
   assert agent(wid,key,{'action':'cancel','request_id':'cancel-before'})[0]==202
   assert agent(wid,key,{**begin,'request_id':'cancel-before'})[0]==409
   status,r=agent(wid,key,begin);assert status==201,(status,r);file_upload=r['upload']
   status,live=owner('/api/w/'+wid+'/board');assert status==200,(status,live)
   status,other=owner('/api/w/'+wid+'/notes',{'title':'Available during upload','text':'independent'});assert status==201,(status,other)
   status,r=agent(wid,key,begin);assert status==200 and r['upload']==file_upload and r['offset']==0,(status,r)
   assert agent(wid,key,{**begin,'name':'different.txt'})[0]==409
   status,r=agent(wid,key,{'action':'chunk','upload':file_upload,'offset':0,'bytes':list(data)});assert status==200,(status,r)
   if protected:
    for file in root.rglob('*'):
     if file.is_file() and not file.is_symlink() and file.stat().st_size<30_000_000:assert data not in file.read_bytes(),str(file)
   status,r=agent(wid,key,begin);assert status==200 and r['offset']==len(data),(status,r)
   status,r=agent(wid,key,{'action':'finish','upload':file_upload});assert status==201,(status,r);fid=r['id']
   status,board=owner('/api/w/'+wid+'/board');assert status==200 and any(x['id']==fid for x in board['objects']),(status,board)
   status,r=agent(wid,key,begin);assert status==201,(status,r);replay=r['upload'];time.sleep(.3)
   status,r=agent(wid,key,{'action':'chunk','upload':replay,'offset':0,'bytes':list(data)});assert status==200,(status,r)
   status,r=agent(wid,key,{'action':'finish','upload':replay});assert status==201 and r['id']==fid and r['replayed'],(status,r)
   race={**begin,'request_id':'finish-revoke'};status,r=agent(wid,key,race);assert status==201,(status,r);race_id=r['upload']
   assert agent(wid,key,{'action':'chunk','upload':race_id,'offset':0,'bytes':list(data)})[0]==200
   (barrier/'enabled').touch();answers=[];thread=threading.Thread(target=lambda:answers.append(agent(wid,key,{'action':'finish','upload':race_id})));thread.start()
   deadline=time.monotonic()+5
   while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
   assert (barrier/'ready').exists();assert owner('/api/agents/'+a['id'],{'name':'a','permissions':{}},'PUT')[0]==200
   (barrier/'resume').touch();thread.join(10);assert not thread.is_alive() and answers[0][0]==403,answers
   assert owner('/api/agents/'+a['id'],{'name':'a','permissions':{wid:{'read':True,'add':True}}},'PUT')[0]==200
   (barrier/'enabled').unlink()
   # Cancel after finish has acquired the operation lease, but before storage commit.
   deadline=time.monotonic()+5;cancelled={**begin,'request_id':'finish-cancel','name':'cancelled.txt'}
   while time.monotonic()<deadline:
    status,r=agent(wid,key,cancelled)
    if status==201:break
    time.sleep(.05)
   assert status==201,(status,r);cancel_id=r['upload'];assert agent(wid,key,{'action':'chunk','upload':cancel_id,'offset':0,'bytes':list(data)})[0]==200
   (commit_barrier/'enabled').touch();cancel_results=[];thread=threading.Thread(target=lambda:cancel_results.append(agent(wid,key,{'action':'finish','upload':cancel_id})));thread.start();deadline=time.monotonic()+5
   while not (commit_barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
   assert (commit_barrier/'ready').exists();assert agent(wid,key,{'action':'cancel','upload':cancel_id})[0]==202;(commit_barrier/'resume').touch();thread.join(10);assert not thread.is_alive() and cancel_results[0][0]==409,cancel_results
   (commit_barrier/'enabled').unlink();status,board=owner('/api/w/'+wid+'/board');assert status==200 and not any(x.get('title')=='cancelled.txt' for x in board['objects']),(status,board)
   if local and protected:
    # Regrant stays blocked while a revoked upload is finishing its commit.
    for name in ("ready", "resume"):
     (commit_barrier/name).unlink(missing_ok=True)
    deadline=time.monotonic()+5;pending={**begin,'request_id':'pending-revoke','name':'pending.txt'}
    while time.monotonic()<deadline:
     status,r=agent(wid,key,pending)
     if status==201:break
     time.sleep(.05)
    assert status==201,(status,r);pending_id=r['upload'];assert agent(wid,key,{'action':'chunk','upload':pending_id,'offset':0,'bytes':list(data)})[0]==200
    (commit_barrier/'enabled').touch();results=[];thread=threading.Thread(target=lambda:results.append(agent(wid,key,{'action':'finish','upload':pending_id})));thread.start();deadline=time.monotonic()+5
    while not (commit_barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
    assert (commit_barrier/'ready').exists();status,value=owner('/api/agents/'+a['id'],{'name':'a','permissions':{}},'PUT');assert status==202,(status,value)
    status,value=owner('/api/agents');assert status==200 and a['id'] in value['pending_ids'],(status,value)
    status,value=owner('/api/agents/'+a['id'],{'name':'a','permissions':{wid:{'read':True,'add':True}}},'PUT');assert status==409,(status,value)
    (commit_barrier/'resume').touch();thread.join(10);assert not thread.is_alive(),results;deadline=time.monotonic()+5
    while time.monotonic()<deadline:
     status,value=owner('/api/agents')
     if a['id'] not in value['pending_ids']:break
     time.sleep(.05)
    assert status==200 and a['id'] not in value['pending_ids'],value
    assert owner('/api/agents/'+a['id'],{'name':'a','permissions':{wid:{'read':True,'add':True}}},'PUT')[0]==200
    (commit_barrier/'enabled').unlink()
   if not local and protected:
    # Hold the worker inside a chunk and verify revocation does not wait for it.
    deadline=time.monotonic()+5;hung={**begin,'request_id':'hung-worker'}
    while time.monotonic()<deadline:
     status,r=agent(wid,key,hung)
     if status==201:break
     time.sleep(.05)
    assert status==201,(status,r);hung_id=r['upload'];(chunk_barrier/'enabled').touch();answers=[]
    thread=threading.Thread(target=lambda:answers.append(agent(wid,key,{'action':'chunk','upload':hung_id,'offset':0,'bytes':list(data)})));thread.start();deadline=time.monotonic()+5
    while not (chunk_barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.01)
    assert (chunk_barrier/'ready').exists();start=time.monotonic();assert owner('/api/agents/'+a['id'],{'name':'a','permissions':{}},'PUT')[0]==200;assert time.monotonic()-start<2
    assert owner('/api/w/default/board')[0]==200
    (chunk_barrier/'resume').touch();thread.join(10);assert not thread.is_alive() and answers[0][0]==409,answers
    (chunk_barrier/'enabled').unlink();assert owner('/api/agents/'+a['id'],{'name':'a','permissions':{wid:{'read':True,'add':True}}},'PUT')[0]==200


   abandoned={**begin,'request_id':'abandoned'};deadline=time.monotonic()+5
   while time.monotonic()<deadline:
    status,r=agent(wid,key,abandoned)
    if status==201:break
    time.sleep(.05)
   assert status==201,(status,r);abandoned_id=r['upload']
   assert agent(wid,key,{'action':'cancel','upload':abandoned_id})[0]==202
   assert agent(wid,key,abandoned)[0]==409

  deadline=time.monotonic()+5;transient_begin=transient('file.txt',3)
  while time.monotonic()<deadline:
   status,r=agent(wid,key,transient_begin)
   if status==201:break
   time.sleep(.05)
  assert status==201,(status,r);uid=r['upload']
  assert agent(wid,b['token'],{'action':'chunk','upload':uid,'offset':0,'bytes':[97]})[0]==409
  assert agent(wid,key,{'action':'chunk','upload':uid,'offset':1,'bytes':[97]})[0]==409
  status,r=agent(wid,key,{'action':'chunk','upload':uid,'offset':0,'bytes':[97,98,99]});assert status==200 and r['offset']==3,(status,r)
  if not local:assert agent(wid,key,{'action':'finish','upload':uid})[0]==501
  assert agent(wid,key,{'action':'cancel','upload':uid})[0]==202
  deadline=time.monotonic()+5;idle_begin=transient('idle.txt',1)
  while time.monotonic()<deadline:
   status,r=agent(wid,key,idle_begin)
   if status==201:break
   time.sleep(.05)
  assert status==201,(status,r);idle=r['upload'];time.sleep(2.4)
  assert agent(wid,key,{'action':'chunk','upload':idle,'offset':0,'bytes':[97]})[0]==409
  deadline=time.monotonic()+5;revoke_begin=transient('file.txt',1)
  while time.monotonic()<deadline:
   status,r=agent(wid,key,revoke_begin)
   if status==201:break
   time.sleep(.05)
  assert status==201,(status,r);uid=r['upload']
  assert owner('/api/agents/'+a['id'],{'name':'a','permissions':{}},'PUT')[0]==200
  assert agent(wid,key,{'action':'chunk','upload':uid,'offset':0,'bytes':[97]})[0]==403
  deadline=time.monotonic()+5
  while time.monotonic()<deadline:
   status,r=agent(wid,b['token'],transient('other.txt',1))
   if status==201:break
   time.sleep(.05)
  assert status==201,(status,r);upload=r['upload'];status,closed=owner('/api/workspaces/'+wid+'/close',{});assert status==200,(status,closed);assert agent(wid,b['token'],{'action':'chunk','upload':upload,'offset':0,'bytes':[97]})[0] in (409,501)
  print('agent upload lifecycle passed:', ('local-protected' if protected else 'local-plain') if local else ('external-protected' if protected else 'external-plain'))
 finally:
  import signal
  try:os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  p.wait(timeout=10)
