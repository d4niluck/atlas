import json,os,pathlib,subprocess,sys,tempfile,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
for binary,flag,expected in [(sys.argv[1],None,0),(sys.argv[2],None,1),(sys.argv[3],None,0),(sys.argv[3],'1',1)]:
 with tempfile.TemporaryDirectory(prefix='atlas-caps-') as tmp:
  env={**os.environ};env.pop('ATLAS_TEST_REQUIRE_NOTE_LEASE',None)
  if flag:env['ATLAS_TEST_REQUIRE_NOTE_LEASE']=flag
  p=subprocess.Popen([binary,'--root',str(pathlib.Path(tmp)/'data'),'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
  try:
   p.stdin.write('owner-test-secret\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
   def request(secret):
    r=urllib.request.Request('http://127.0.0.1:'+port+'/api/capabilities',headers={'X-Atlas-Token':secret})
    try:
     with urllib.request.urlopen(r,timeout=10) as x:return x.status,json.load(x)
    except urllib.error.HTTPError as e:return e.code,json.load(e)
   assert request('wrong')[0]==403
   status,result=request('owner-test-secret');assert status==200 and result=={'owner_edit_protocol':expected},(binary,flag,status,result)
  finally:p.terminate();p.wait(timeout=10)
