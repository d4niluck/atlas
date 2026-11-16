import json,os,pathlib,signal,socket,sqlite3,subprocess,sys,tempfile,time,urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='ats-',dir='/private/tmp') as tmp:
 root=pathlib.Path(tmp).resolve();sockdir=root/'sock';sockdir.mkdir(mode=0o700)
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir)),start_new_session=True)
 stopped=[]
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None):
   data=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=data,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   with urllib.request.urlopen(req,timeout=10) as response:return json.load(response)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  profile=owner('/api/agents',{'name':'fixture'});key=profile['token'];aid=profile['id'];spaces=[];dbpaths=[]
  for i in range(4):
   disk=root/('disk'+str(i));disk.mkdir();cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})['capability'];made=owner('/api/workspaces/create-external',{'capability':cap,'name':'Fixture'+str(i),'protected':False});wid=owner('/api/workspaces/attach',{'capability':made['capability']})['id'];owner('/api/w/'+wid+'/notes',{'title':'fixture','text':'unicorn report'});spaces.append(wid);dbpaths.append(pathlib.Path(made['path'])/'atlas.sqlite3')
  owner('/api/agents/'+aid,{'name':'fixture','permissions':{wid:{'read':True,'lexical':True} for wid in spaces}},'PUT')
  for wid in spaces:
   body=json.dumps({'query':'unicorn','mode':'lexical'}).encode()
   with socket.socket(socket.AF_UNIX) as conn:
    conn.settimeout(10);conn.connect(str(sockdir/'agent.sock'));conn.sendall((f'POST /api/agent/{wid}/search HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n').encode()+body);data=b''
    while True:
     b=conn.recv(65536)
     if not b:break
     data+=b
   assert data.startswith(b'HTTP/1.1 200'),data[:200]
  for _ in range(100):
   events=owner('/api/agent-activity')['events']
   if sum(e.get('request',{}).get('query')=='unicorn' for e in events)==4:break
   time.sleep(.05)
  assert sum(e.get('request',{}).get('query')=='unicorn' for e in events)==4,events
  workers=[int(x) for x in subprocess.check_output(['pgrep','-P',str(p.pid)],text=True).split()];assert len(workers)==4,workers
  for worker in workers:os.kill(worker,signal.SIGSTOP);stopped.append(worker)
  start=time.monotonic();owner('/api/agent-activity',method='DELETE');elapsed=time.monotonic()-start;assert elapsed<2,(elapsed,'clear blocked by four suspended workers')
  assert not owner('/api/agent-activity')['events']
  for worker in stopped:os.kill(worker,signal.SIGCONT)
  stopped.clear()
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   events=owner('/api/agent-activity')['events'];counts=[]
   for path in dbpaths:
    with sqlite3.connect(path,timeout=2) as db:
     exists=db.execute("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='agent_activity_details')").fetchone()[0]
     counts.append(db.execute('SELECT COUNT(*) FROM agent_activity_details').fetchone()[0] if exists else 0)
   if not events and counts==[0,0,0,0]:break
   time.sleep(.1)
  assert not events and counts==[0,0,0,0],(events,counts)
  print('four suspended workers: clear',round(elapsed,3),'s; recovery passed')
 finally:
  for worker in stopped:
   try:os.kill(worker,signal.SIGCONT)
   except ProcessLookupError:pass
  try:os.killpg(p.pid,signal.SIGTERM)
  except ProcessLookupError:pass
  p.wait(timeout=15)
