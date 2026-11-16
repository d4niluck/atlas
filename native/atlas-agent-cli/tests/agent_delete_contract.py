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


  # An agent may not delete a note held by another editor.
  held=run('edit','default',note['id'],'acquire')['lease']
  denied=subprocess.run([cli,'--key-file',str(key),'delete','default',note['id']],capture_output=True,text=True,timeout=10)
  assert denied.returncode!=0
  assert run('read','default',note['id'])['text']=='Updated from CLI'
  assert run('edit','default',note['id'],'release',held)['ok']
  before_signal=owner('/api/w/default/agent-board-signal')['signal']
  deleted=run('delete','default',note['id']);assert deleted['ok']
  assert owner('/api/w/default/agent-board-signal')['signal']==before_signal+1
  assert any(x['id']==note['id'] for x in owner('/api/w/default/trash')['objects'])
  owner('/api/w/default/trash/'+note['id']+'/restore',{},'POST')
  assert run('read','default',note['id'])['text']=='Updated from CLI'

 finally:
  proc.terminate();proc.wait(timeout=10)
print('agent delete contract passed')
