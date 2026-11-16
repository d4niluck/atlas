import hashlib,json,os,pathlib,socket,subprocess,sys,tempfile,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='ata-',dir='/private/tmp') as tmp:
 root=pathlib.Path(tmp).resolve();sockdir=root/'sock';sockdir.mkdir(mode=0o700)
 env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_AGENT_UPLOAD='1')
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None):
   data=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=data,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   with urllib.request.urlopen(req,timeout=10) as response:return json.load(response)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  wid='default'
  if len(sys.argv)>2 and sys.argv[2]=='protected':wid=owner('/api/workspaces',{'name':'protected','protected':True,'password':'test-password'})['id']
  a=owner('/api/agents',{'name':'fixture'});key=a['token'];aid=a['id'];owner('/api/agents/'+aid,{'name':'fixture','permissions':{wid:{'read':True,'add':True,'edit':True,'lexical':True}}},'PUT')
  def agent(method,endpoint,body=None):
   raw=b'' if body is None else json.dumps(body).encode()
   with socket.socket(socket.AF_UNIX) as c:
    c.settimeout(15);c.connect(str(sockdir/'agent.sock'));c.sendall((f'{method} /api/agent/{wid}/{endpoint} HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(raw)}\r\n\r\n').encode()+raw);data=b''
    while True:
     b=c.recv(65536)
     if not b:break
     data+=b
   head,data=data.split(b'\r\n\r\n',1);return int(head.split()[1]),json.loads(data)
  secret='secret body content';status,created=agent('POST','notes',{'title':'Fixture','text':secret,'request_id':'fixture-request'});assert status==201,(status,created);nid=created['id']
  status,_=agent('GET','notes/'+nid+'?offset=0&limit=12');assert status==200,status
  status,lock=agent('POST','notes/'+nid+'/edit',{'action':'acquire'});assert status==200,(status,lock)
  status,_=agent('POST','notes/'+nid+'/edit',{'action':'commit','lease':lock['lease'],'title':'New title','text':secret});assert status==200,status
  status,_=agent('POST','notes/'+nid+'/edit',{'action':'release','lease':lock['lease']});assert status==200,status
  payload=b'private-file-content';digest=hashlib.sha256(payload).hexdigest();status,beg=agent('POST','uploads',{'action':'begin','name':'fixture.txt','size':len(payload),'request_id':'upload-fixture','digest':digest});assert status==201,(status,beg)
  status,_=agent('POST','uploads',{'action':'chunk','upload':beg['upload'],'offset':0,'bytes':list(payload)});assert status==200,status
  status,done=agent('POST','uploads',{'action':'finish','upload':beg['upload']});assert status==201,(status,done)
  status,_=agent('POST','objects/'+nid+'/delete',{});assert status==200,status
  owner('/api/agents/'+aid,{'name':'fixture','permissions':{}},'PUT')
  status,_=agent('GET','notes/'+nid);assert status==403,status
  end=time.monotonic()+10
  while True:
   log=owner('/api/agent-activity');events=log['events'];actions={e['action'] for e in events if e.get('request') is not None}
   if {'create','read','edit','upload','delete'}<=actions:break
   assert time.monotonic()<end,(actions,events);time.sleep(.05)
  find=lambda action,step=None:next(e for e in events if e['action']==action and e.get('request') is not None and (step is None or e.get('request',{}).get('action')==step))
  assert find('create')['request']['title']=='Fixture'
  assert find('read')['request']['object_id']==nid and find('read')['request']['limit']==12
  assert find('edit','commit')['request']['object_id']==nid
  assert find('delete')['request']['object_id']==nid
  assert find('upload','begin')['request']['name']=='fixture.txt'
  assert find('upload','finish')['response']['id']==done['id']
  assert len([e for e in events if e['action']=='upload'])==2
  denied=[e for e in events if e['status']==403];assert len(denied)==1 and denied[0].get('request') is None,denied
  dump=json.dumps(events);assert secret not in dump and payload.decode() not in dump and key not in dump
 finally:p.terminate();p.wait(timeout=10)
print('agent activity actions passed',sys.argv[2:] or ['local'])
