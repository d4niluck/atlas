import json, os, pathlib, subprocess, sys, tempfile, urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
backend, cli = sys.argv[1:]
with tempfile.TemporaryDirectory(prefix='atlas-sock-',dir='/tmp') as t:
 socket_dir=pathlib.Path(t)/'agent'
 os.environ['ATLAS_TEST_AGENT_SOCKET_DIR']=str(socket_dir)
 os.environ['ATLAS_TEST_REQUIRE_NOTE_LEASE']='1'
 root=pathlib.Path(t)/'root'; proc=subprocess.Popen([backend,'--root',str(root),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  proc.stdin.write('owner-test-key\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),proc.stderr.read()
  def owner(path,data=None,method=None):
   req=urllib.request.Request(f'http://127.0.0.1:{port}{path}',None if data is None else json.dumps(data).encode(),{'X-Atlas-Token':'owner-test-key','Content-Type':'application/json'},method=method)
   with urllib.request.urlopen(req,timeout=5) as r:return json.load(r)
  try:
   urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{port}/api/agent/workspaces',headers={'X-Atlas-Agent-Token':'bad'}),timeout=3)
   raise AssertionError('agent route exposed on TCP')
  except urllib.error.HTTPError as exc: assert exc.code == 404
  profile=owner('/api/agents',{'name':'test'});key=pathlib.Path(t)/'key';key.write_text(profile['token']);key.chmod(0o600)
  owner('/api/agents/'+profile['id'],{'name':'test','permissions':{'default':{'read':True,'lexical':True,'edit':True,'add':True}}},'PUT')
  note=owner('/api/w/default/notes',{'title':'Hi','text':'unique-abc'})
  def run(*args):return json.loads(subprocess.check_output([cli,'--key-file',str(key),'--json',*args],text=True))
  assert run('workspaces')['workspaces'][0]['id']=='default'
  assert run('read','default',note['id'])['text']=='unique-abc'
  assert run('search','default','unique')['results']
  created_text=pathlib.Path(t)/'created.md';created_text.write_text('Новая заметка от агента')
  created=run('create','default','Созданная',str(created_text),'cli-create-id')
  repeated=run('create','default','Созданная',str(created_text),'cli-create-id')
  assert created['id']==repeated['id'] and repeated['replayed'] is True
  assert run('read','default',created['id'])['text']=='Новая заметка от агента'

  lease=run('edit','default',note['id'],'acquire')['lease']
  text_file=pathlib.Path(t)/'edited.md';text_file.write_text('Updated from CLI')
  assert run('edit','default',note['id'],'commit',lease,'Edited',str(text_file))['ok']
  assert run('edit','default',note['id'],'release',lease)['ok']
  assert run('read','default',note['id'])['text']=='Updated from CLI'
  human=subprocess.check_output([cli,'--key-file',str(key),'read','default',note['id']],text=True)
  assert 'Edited' in human and 'Updated from CLI' in human and not human.startswith('{')


  giant=owner('/api/w/default/notes',{'title':'Big','text':'x'*8500000})
  first=run('read','default',giant['id']);assert len(first['text'])==4000 and first['next_offset']==4000
  second=json.loads(subprocess.check_output([cli,'--key-file',str(key),'--json','--offset','4000','--limit','1000','read','default',giant['id']],text=True));assert len(second['text'])==1000 and second['next_offset']==5000
  owner('/api/agents/'+profile['id'],{'name':'test','permissions':{'default':{'read':False}}},'PUT')
  revoked=subprocess.run([cli,'--key-file',str(key),'--json','read','default',note['id']],capture_output=True,text=True,timeout=10);assert revoked.returncode!=0 and '403' in revoked.stderr,(revoked.stdout,revoked.stderr)
  owner('/api/agents/'+profile['id'],{'name':'test','permissions':{'default':{'read':True,'lexical':True,'edit':True,'add':True}}},'PUT')

  # Slow request headers and bodies must release bounded front-end slots.
  import socket, time
  for partial in [b'GET /api/agent/workspaces HTTP/1.1\r\nHost: ', b'POST /api/agent/default/search HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Atlas-Agent-Token: x\r\nContent-Length: 12\r\n\r\n{']:
   held=[]
   for _ in range(16):
    c=socket.socket(socket.AF_UNIX);c.connect(str(socket_dir/'agent.sock'));c.sendall(partial);held.append(c)
   time.sleep(4)
   assert run('workspaces')['workspaces']
   for c in held:c.close()

  other=subprocess.Popen([backend,'--root',str(pathlib.Path(t)/'other'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  other.stdin.write('other-key\n');other.stdin.flush()
  try:
   other_port=other.stdout.readline().strip();assert other_port.isdigit()
   req=urllib.request.Request(f'http://127.0.0.1:{other_port}/api/agents',headers={'X-Atlas-Token':'other-key'})
   with urllib.request.urlopen(req,timeout=5) as r: assert json.load(r)['socket_available'] is False
   assert run('workspaces')['workspaces']
  finally: other.terminate();other.wait(timeout=10)
 finally:
  proc.terminate();proc.wait(timeout=10)
 # Stale socket from terminated process must be usable after restart.
 proc=subprocess.Popen([backend,'--root',str(root),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  proc.stdin.write('owner-test-key\n');proc.stdin.flush();assert proc.stdout.readline().strip().isdigit();assert run('workspaces')['workspaces']
 finally: proc.terminate();proc.wait(timeout=10)
print('socket e2e passed')
