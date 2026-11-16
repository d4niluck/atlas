import concurrent.futures,json,os,pathlib,socket,subprocess,sys,tempfile,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-activity-e2e-') as t:
 root=pathlib.Path(t).resolve();sockdir=root/'sock';sockdir.mkdir(mode=0o700)
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir)))
 try:
  p.stdin.write('owner-secret\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None,token='owner-secret'):
   raw=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=raw,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
   try:
    with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  profile=owner('/api/agents',{'name':'A'})[1];key=profile['token'];aid=profile['id'];secret='sensitive-unique-content'
  note=owner('/api/w/default/notes',{'title':'private-title','text':secret})[1]['id'];path='/api/agent/default/notes/'+note
  owner('/api/agents/'+aid,{'name':'A','permissions':{'default':{'read':True}}},'PUT')
  def agent(target=path):
   with socket.socket(socket.AF_UNIX) as s:
    s.settimeout(10);s.connect(str(sockdir/'agent.sock'));s.sendall((f'GET {target} HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\n\r\n').encode());data=b''
    while True:
     b=s.recv(65536)
     if not b:break
     data+=b
   return int(data.split(b' ')[1])
  with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:assert list(pool.map(lambda _:agent(),range(24)))==[200]*24
  assert owner('/api/agent-activity',token='wrong')[0]==403
  status,log=owner('/api/agent-activity');assert status==200 and len(log['events'])==24 and log['failed']==0,(status,len(log.get('events',[])),log.get('failed'))
  assert all(e['agent_id']==aid and e['workspace_id']=='default' and e['action']=='read' and e['status']==200 for e in log['events'])
  assert secret not in json.dumps(log) and key not in json.dumps(log) and 'private-title' not in json.dumps(log)
  big=owner('/api/w/default/notes',{'title':'Large','text':'x'*8000100})[1]['id'];assert agent('/api/agent/default/notes/'+big)==413
  assert owner('/api/agent-activity')[1]['events'][0]['status']==413
  assert owner('/api/agent-activity-settings',{'workspace_id':'default','enabled':False},'PUT')[0]==200
  assert agent()==200 and len(owner('/api/agent-activity')[1]['events'])==25
  assert owner('/api/agent-activity',method='DELETE')[0]==200 and not owner('/api/agent-activity')[1]['events']
 finally:
  p.terminate();p.wait(timeout=10)
print('agent activity e2e passed')
