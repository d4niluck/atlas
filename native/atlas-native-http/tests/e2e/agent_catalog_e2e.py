import json,os,pathlib,socket,subprocess,sys,tempfile,time,urllib.error,urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='cat-e2e-',dir='/tmp') as tmp:
 root=pathlib.Path(tmp);sockdir=root/'sock';sockdir.mkdir(mode=0o700)
 env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir))
 proc=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 try:
  proc.stdin.write('owner-secret\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),proc.stderr.read()
  def owner(path,value=None,method=None):
   data=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=data,method=method,headers={'X-Atlas-Token':'owner-secret','Content-Type':'application/json'})
   try:
    with urllib.request.urlopen(req,timeout=20) as response:return response.status,json.load(response)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  def api(wid,payload):
   body=json.dumps(payload).encode()
   with socket.socket(socket.AF_UNIX) as conn:
    conn.settimeout(20);conn.connect(str(sockdir/'agent.sock'));conn.sendall((f'POST /api/agent/{wid}/objects/list HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(body)}\r\nConnection: close\r\n\r\n').encode()+body);data=b''
    while True:
     part=conn.recv(65536)
     if not part:break
     data+=part
   header,body=data.split(b'\r\n\r\n',1);return int(header.split()[1]),json.loads(body)
  def must(method_result,code=200):
   status,value=method_result;assert status==code,(status,value);return value
  for _ in range(200):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.05)
  profile=must(owner('/api/agents',{'name':'catalog-tester'}),201);key=profile['token'];aid=profile['id'];perms={}
  extdir=root/'external';extdir.mkdir();spaces=[('local','default')]
  p=must(owner('/api/workspaces',{'name':'protected','protected':True,'password':'test-password'}),201);spaces.append(('protected',p['id']))
  for encrypted in (False,True):
   d=extdir/('protected' if encrypted else 'plain');d.mkdir();cap=must(owner('/api/workspaces/authorize',{'operation':'export','path':str(d)}))['capability'];body={'capability':cap,'name':'external','protected':encrypted}
   if encrypted:body['password']='test-password'
   made=must(owner('/api/workspaces/create-external',body),201);attach={'capability':made['capability']}
   if encrypted:attach.update(password='test-password',test_protected=True)
   w=must(owner('/api/workspaces/attach',attach));spaces.append(('external-protected' if encrypted else 'external',w['id']))
  for label,wid in spaces:
   prefix='/api/w/'+wid
   notes=[]
   for title,text in [('Машинное обучение','bodysecret'),('Машинное зрение','unrelated'),('Скрытое обучение','hidden'),('Удалённое обучение','deleted'),('Пустая','')]:
    notes.append(must(owner(prefix+'/notes',{'title':title,'text':text}),201)['id'])
   must(owner(prefix+'/objects/'+notes[2]+'/search-visibility',{'excluded':True},'PUT'))
   must(owner(prefix+'/objects/'+notes[3],method='DELETE'))
   def setperm(rights):
    perms[wid]=rights;must(owner('/api/agents/'+aid,{'name':'catalog-tester','permissions':perms},'PUT'))
   setperm({'read':True})
   first=must(api(wid,{'offset':0,'limit':2,'query':None}));second=must(api(wid,{'offset':2,'limit':2,'query':None}));allids=[x['id'] for x in first['items']+second['items']]
   assert first['total']==4 and first['next_offset']==2 and second['next_offset'] is None,(label,first,second)
   assert len(set(allids))==4 and notes[2] in allids and notes[3] not in allids,(label,allids)
   assert all(set(x)=={'id','title','kind'} for x in first['items']+second['items']),(label,first,second)
   assert 'bodysecret' not in json.dumps(first,ensure_ascii=False),(label,first)
   assert api(wid,{'offset':0,'limit':10,'query':'обучение'})[0]==403,label
   setperm({'read':True,'lexical':True})
   found=must(api(wid,{'offset':0,'limit':10,'query':'ОБУЧЕНИЕ'}));assert [x['id'] for x in found['items']]==[notes[0]],(label,found)
   body=must(api(wid,{'offset':0,'limit':10,'query':'bodysecret'}));assert body['total']==0,(label,body)
   multi=must(api(wid,{'offset':0,'limit':1,'query':'ОБУЧЕНИЕ ЗРЕНИЕ'}));assert multi['total']==2 and multi['next_offset']==1,(label,multi)
   ranked=must(api(wid,{'offset':0,'limit':1,'query':'МАШИННОЕ ОБУЧЕНИЕ'}));assert ranked['total']==2 and ranked['items'][0]['id']==notes[0],(label,ranked)
   deep=must(api(wid,{'offset':10001,'limit':2,'query':None}));assert deep['items']==[] and deep['total']==4 and deep['next_offset'] is None,(label,deep)
   bad=api(wid,{'offset':0,'limit':1000,'query':None});assert bad[0]==400,(label,bad)
   setperm({'read':False});assert api(wid,{'offset':0,'limit':2,'query':None})[0]==403,label
   print('PASS',label,flush=True)
  events=must(owner('/api/agent-activity'))['events']
  for label,wid in spaces:
   found={(event['action'],event['status']) for event in events if event['workspace_id']==wid}
   assert ('list',200) in found and ('search_titles',200) in found,(label,found)
  print('PASS activity history list/search_titles',flush=True)
 finally:
  proc.terminate()
  try:proc.wait(timeout=10)
  except subprocess.TimeoutExpired:proc.kill();proc.wait()
