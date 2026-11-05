import json,os,subprocess,sys,tempfile,signal
from pathlib import Path
from urllib.request import Request,urlopen
from urllib.error import HTTPError
b=sys.argv[1]; t='synthetic-token'
with tempfile.TemporaryDirectory() as d:
 legacy=Path(d)/'legacy';legacy.mkdir();root=legacy/'.demo-workspace-workspaces';w=legacy/'demo-workspace'; sentinel=legacy/'unrelated.txt';sentinel.write_text('keep-me');p=None
 def start():
  global p,url
  p=subprocess.Popen([b,'--root',str(root),'--default-workspace',str(w),'--port','0','--token-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True);p.stdin.write(t+'\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit(),port;url='http://127.0.0.1:'+port
 def stop():
  if p.poll() is None:os.killpg(p.pid,signal.SIGTERM)
  p.wait(timeout=10)
 def call(path,method='GET',data=None):
  q=Request(url+path,method=method,data=None if data is None else json.dumps(data).encode(),headers={'X-Atlas-Token':t,'Content-Type':'application/json'})
  try:
   with urlopen(q,timeout=10) as r:return r.status,json.loads(r.read())
  except HTTPError as e:return e.code,json.loads(e.read())
 try:
  start();s,v=call('/api/w/default/notes','POST',{'title':'synthetic','text':'keep-safe'});assert s==201,(s,v);stop();(w/'.atlas-owned').unlink();(w/'.atlas-workspace.json').write_text('{"name":"legacy"}');start();s,v=call('/api/workspaces/default','DELETE');print('legacy-delete:',s,v);assert sentinel.read_text()=='keep-me';assert s==200,(s,v);stop();start();s,v=call('/api/workspaces');assert all(x['id']!='default' for x in v['workspaces']);assert sentinel.read_text()=='keep-me';print('PASS legacy default and unrelated file')
 finally:stop()
