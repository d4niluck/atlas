import concurrent.futures,json,os,pathlib,subprocess,sys,tempfile,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
for mode in ('local','protected','external','protected-external'):
 with tempfile.TemporaryDirectory(prefix='atlas-commit-expiry-') as tmp:
  root=pathlib.Path(tmp);barrier=root/'barrier';barrier.mkdir();env={**os.environ,'ATLAS_TEST_REQUIRE_NOTE_LEASE':'1','ATLAS_TEST_EDIT_TTL_MS':'200','ATLAS_TEST_EDIT_COMMIT_BARRIER':str(barrier)}
  p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
  try:
   p.stdin.write('owner-test-secret\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
   def req(path,body=None,method=None):
    data=None if body is None else json.dumps(body).encode();r=urllib.request.Request('http://127.0.0.1:'+port+path,data=data,headers={'X-Atlas-Token':'owner-test-secret','Content-Type':'application/json'},method=method)
    try:
     with urllib.request.urlopen(r,timeout=10) as x:return x.status,json.load(x)
    except urllib.error.HTTPError as x:return x.code,json.load(x)
   wid='default'
   if mode=='protected':
    status,w=req('/api/workspaces',{'name':'Protected','protected':True,'password':'test-password'});assert status==201,(mode,status,w);wid=w['id']
   if 'external' in mode:
    disk=root/'disk';disk.mkdir();status,c=req('/api/workspaces/authorize',{'operation':'export','path':str(disk)});assert status==200
    protected=mode=='protected-external';status,w=req('/api/workspaces/create-external',{'capability':c['capability'],'name':'External','protected':protected,'password':'test-password'});assert status==201,(mode,status,w)
    status,w=req('/api/workspaces/attach',{'capability':w['capability'],'password':'test-password','test_protected':protected});assert status==200,(mode,status,w);wid=w['id']
   prefix='/api/w/'+wid;status,n=req(prefix+'/notes',{'title':'Title','text':'Old'});assert status==201,(mode,status,n);note=prefix+'/notes/'+n['id'];status,lease=req(note+'/edit-lease',{'action':'acquire'});assert status==200,(mode,status,lease)
   with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
    commit=pool.submit(req,note+'/edit-commit',{'lease':lease['lease'],'title':'New','text':'Committed'})
    until=time.monotonic()+8
    while not (barrier/'ready').exists() and time.monotonic()<until:time.sleep(.01)
    assert (barrier/'ready').exists(),mode
    time.sleep(.3)
    second=pool.submit(req,note+'/edit-lease',{'action':'acquire'})
    time.sleep(.15);assert not second.done(),mode
    (barrier/'go').write_text('go');status,result=commit.result(timeout=8);assert status==200,(mode,status,result)
    status,new=second.result(timeout=8);assert status==200,(mode,status,new)
   status,result=req(note,None);assert status==200 and result['text']=='Committed',(mode,status,result)
   assert req(note+'/edit-commit',{'lease':lease['lease'],'title':'Stale','text':'Stale'})[0]==409
  finally:
   p.terminate();p.wait(timeout=10)
