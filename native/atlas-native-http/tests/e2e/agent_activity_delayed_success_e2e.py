import json, os, pathlib, socket, subprocess, sys, tempfile, time, urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-details-e2e-') as tmp:
 root=pathlib.Path(tmp).resolve();sockdir=root/'sock';sockdir.mkdir(mode=0o700);barrier=root/'barrier';barrier.mkdir()
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_ACTIVITY_WORKER_BARRIER=str(barrier)))
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None):
   body=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=body,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   with urllib.request.urlopen(req,timeout=10) as response:return json.load(response)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  profile=owner('/api/agents',{'name':'fixture'});key=profile['token'];aid=profile['id']
  disk=root/'disk';disk.mkdir()
  cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})['capability']
  made=owner('/api/workspaces/create-external',{'capability':cap,'name':'External','protected':False})
  wid=owner('/api/workspaces/attach',{'capability':made['capability']})['id']
  owner('/api/w/'+wid+'/notes',{'title':'fixture','text':'unicorn report'})
  owner('/api/agents/'+aid,{'name':'fixture','permissions':{wid:{'read':True,'lexical':True}}},'PUT')
  (barrier/'enabled').touch()
  body=json.dumps({'query':'unicorn','mode':'lexical'}).encode()
  with socket.socket(socket.AF_UNIX) as conn:
   conn.settimeout(10);conn.connect(str(sockdir/'agent.sock'));conn.sendall((f'POST /api/agent/{wid}/search HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n').encode()+body);data=b''
   while True:
    b=conn.recv(65536)
    if not b:break
    data+=b
  assert data.startswith(b'HTTP/1.1 200'),data[:150]
  for _ in range(200):
   if (barrier/'ready').exists():break
   time.sleep(.01)
  assert (barrier/'ready').exists()
  time.sleep(.4)
  assert owner('/api/agent-activity')['failed']==0
  (barrier/'resume').touch()
  for _ in range(200):
   if (barrier/'done').exists():break
   time.sleep(.01)
  assert (barrier/'done').exists()
  for _ in range(100):
   events=owner('/api/agent-activity')['events']
   if events and events[0].get('request',{}).get('query')=='unicorn':break
   time.sleep(.05)
  assert events[0]['request']['query']=='unicorn',events
  assert key not in json.dumps(events)
  assert b'unicorn' not in (root/'data/.atlas-agent-profiles.sqlite3').read_bytes()

 finally:
  p.terminate();p.wait(timeout=10)
print("delayed external success is not counted as failure")
