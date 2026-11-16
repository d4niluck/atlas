import json, os, pathlib, socket, subprocess, sys, tempfile, time, urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-details-e2e-') as tmp:
 root=pathlib.Path(tmp).resolve();sockdir=root/'sock';sockdir.mkdir(mode=0o700);barrier=root/'barrier';barrier.mkdir();(barrier/'enabled').touch()
 p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_ACTIVITY_FLUSH_BARRIER=str(barrier)))
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None):
   body=None if value is None else json.dumps(value).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=body,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   with urllib.request.urlopen(req,timeout=10) as response:return json.load(response)
  for _ in range(100):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  profile=owner('/api/agents',{'name':'fixture'});key=profile['token'];aid=profile['id']
  owner('/api/w/default/notes',{'title':'fixture','text':'unicorn report'})
  owner('/api/agents/'+aid,{'name':'fixture','permissions':{'default':{'read':True,'lexical':True}}},'PUT')
  body=json.dumps({'query':'unicorn','mode':'lexical'}).encode()
  with socket.socket(socket.AF_UNIX) as conn:
   conn.settimeout(10);conn.connect(str(sockdir/'agent.sock'));conn.sendall((f'POST /api/agent/default/search HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n').encode()+body);data=b''
   while True:
    b=conn.recv(65536)
    if not b:break
    data+=b
  assert data.startswith(b'HTTP/1.1 200'),data[:150]
  for _ in range(200):
   if (barrier/'ready').exists():break
   time.sleep(.02)
  assert (barrier/'ready').exists(),'flush barrier not reached'
  owner('/api/agent-activity',method='DELETE')
  assert owner('/api/agent-activity')['events']==[]
  (barrier/'resume').touch()
  for _ in range(200):
   if (barrier/'done').exists():break
   time.sleep(.02)
  assert (barrier/'done').exists(),'flush did not finish'
  assert not owner('/api/agent-activity')['events']
  import sqlite3
  with sqlite3.connect(root/'data/.default/atlas.sqlite3') as db:
   exists=db.execute("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='agent_activity_details')").fetchone()[0]
   if exists: assert db.execute('SELECT COUNT(*) FROM agent_activity_details').fetchone()[0]==0,'delayed flush restored cleared details'

 finally:
  p.terminate();p.wait(timeout=10)
print('delayed flush after history clear passed')
