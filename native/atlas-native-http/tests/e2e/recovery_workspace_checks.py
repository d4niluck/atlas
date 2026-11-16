import fcntl,json,os,pathlib,shutil,subprocess,sys,tempfile,urllib.error,urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-recovery-checks-',dir='/tmp') as t:
 root=pathlib.Path(t)/'data';p=subprocess.Popen([sys.argv[1],'--root',str(root),'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  p.stdin.write('fixture-owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def get(path):
   req=urllib.request.Request('http://127.0.0.1:'+port+path,headers={'X-Atlas-Token':'fixture-owner'})
   try:
    with urllib.request.urlopen(req,timeout=8) as r:return r.status,json.load(r)
   except urllib.error.HTTPError as e:return e.code,json.load(e)
  checks=['agent-create-recovered','agent-create-pending','agent-file-pending']
  assert any(w['id']=='default' for w in get('/api/workspaces')[1]['workspaces'])
  for _ in range(3):
   for check in checks:
    code,body=get('/api/w/default/'+check);assert code==200,(check,code,body)
  shutil.move(root/'.default',root/'.removed-for-test')
  locked_code,locked_body=get('/api/w/default/agent-file-pending');assert locked_code>=400,(locked_code,locked_body)
  code,body=get('/api/workspaces');assert code==200 and not any(w['id']=='default' for w in body['workspaces']),(code,body)
  print('recovery checks and absent default passed; missing workspace recovery error:',locked_code,locked_body)
 finally:p.terminate();p.wait(timeout=10)
