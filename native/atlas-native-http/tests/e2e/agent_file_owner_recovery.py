"""Owner recovery HTTP test; seed a crash journal because plain upload finish is experimental."""
import json, os, pathlib, signal, sqlite3, subprocess, sys, tempfile, urllib.request, urllib.error, uuid
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-file-recovery-',dir='/private/tmp') as tmp:
 root=pathlib.Path(tmp); disk=root/'disk';disk.mkdir(); kind=sys.argv[2]; p=subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()),'--root',str(root/'data'),'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True)
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit(),p.stderr.read()
  def owner(path,data=None,method=None):
   b=None if data is None else json.dumps(data).encode();req=urllib.request.Request('http://127.0.0.1:'+port+path,data=b,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   try:
    with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  wid='default';source=root/'data/.default';made=None
  if kind=='external':
   cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})[1]['capability'];status,made=owner('/api/workspaces/create-external',{'capability':cap,'name':'External','protected':False});assert status==201,(status,made);source=pathlib.Path(made['path'])
  oid=str(uuid.uuid4());f=source/'files'/oid/'partial.bin';f.parent.mkdir();f.write_bytes(b'partial fixture content');st=f.stat()
  db=sqlite3.connect(source/'atlas.sqlite3');db.execute('CREATE TABLE IF NOT EXISTS agent_file_intents(actor TEXT NOT NULL,request_id TEXT NOT NULL,fingerprint TEXT NOT NULL,object_id TEXT NOT NULL,name TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,PRIMARY KEY(actor,request_id))');db.execute('INSERT INTO agent_file_intents VALUES(?,?,?,?,?,?,?)',('agent:test','req','fixture',oid,f.name,st.st_dev,st.st_ino));db.commit();db.close()
  if made:
   status,value=owner('/api/workspaces/attach',{'capability':made['capability']});assert status==200,(status,value);wid=value['id']
  status,value=owner(f'/api/w/{wid}/agent-file-pending');assert status==200 and len(value['pending'])==1,(status,value)
  status,value=owner(f'/api/w/{wid}/agent-file-cancel',{'actor':'agent:test','request_id':'req'});assert status==200 and value['cancelled'] and value['object_id']==oid,(status,value)
  status,value=owner(f'/api/w/{wid}/agent-file-pending');assert status==200 and value['pending']==[],(status,value)
  status,board=owner(f'/api/w/{wid}/board');assert status==200 and any(x['id']==oid for x in board['objects']),(status,board)
  assert f.read_bytes()==b'partial fixture content'
  if kind=='external':
   dest=root/'export';dest.mkdir();cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(dest)})[1]['capability']
   status,value=owner('/api/workspaces/export',{'capability':cap,'ids':[wid]});assert status==200,(status,value)
   folder=pathlib.Path(value.get('path') or dest/value['folder']);files=list(folder.rglob('partial.bin'));assert len(files)==1 and files[0].read_bytes()==b'partial fixture content'
   cap=owner('/api/workspaces/authorize',{'operation':'import','path':str(folder)})[1]['capability'];status,value=owner('/api/workspaces/import',{'capability':cap});assert status==201,(status,value)
   copied=value['workspaces'][0]['id'];status,value=owner(f'/api/workspaces/{copied}/protection',{'protected':True,'password':'test-password'},'PATCH');assert status==200,(status,value)
   status,value=owner(f'/api/workspaces/{copied}/unlock',{'password':'test-password'});assert status==200,(status,value)
   req=urllib.request.Request(f'http://127.0.0.1:{port}/api/w/{copied}/files/{oid}/content',headers={'X-Atlas-Token':'owner'})
   with urllib.request.urlopen(req,timeout=10) as response:assert response.read()==b'partial fixture content'
  if kind=='local':
   status,value=owner(f'/api/workspaces/{wid}/close',{});assert status==200,(status,value)
   status,value=owner(f'/api/workspaces/{wid}/protection',{'protected':True,'password':'test-password'},'PATCH');assert status==200,(status,value)
   for path in source.rglob('*'):
    if path.is_file():assert b'partial fixture content' not in path.read_bytes(),path
   status,value=owner(f'/api/workspaces/{wid}/unlock',{'password':'test-password'});assert status==200,(status,value)
   status,board=owner(f'/api/w/{wid}/board');assert status==200 and any(x['id']==oid for x in board['objects']),(status,board)
   req=urllib.request.Request(f'http://127.0.0.1:{port}/api/w/{wid}/files/{oid}/content',headers={'X-Atlas-Token':'owner'})
   with urllib.request.urlopen(req,timeout=10) as response:assert response.read()==b'partial fixture content'
  print('owner file recovery passed:',kind)
 finally:
  try:os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  p.wait(timeout=10)
