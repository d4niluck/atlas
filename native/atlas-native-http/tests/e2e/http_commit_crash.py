import concurrent.futures,json,os,pathlib,signal,subprocess,sys,tempfile,time,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
for mode in ('local','protected','external','protected-external'):
 with tempfile.TemporaryDirectory(prefix='atlas-commit-crash-') as tmp:
  root=pathlib.Path(tmp);barrier=root/'barrier';barrier.mkdir();proc=None
  def start(use_barrier):
   global proc,base
   env={**os.environ,'ATLAS_TEST_REQUIRE_NOTE_LEASE':'1'}
   if use_barrier:env['ATLAS_TEST_EDIT_COMMIT_BARRIER']=str(barrier)
   proc=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env,start_new_session=True);proc.stdin.write('owner-test-secret\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit();base='http://127.0.0.1:'+port
  def req(path,body=None,method=None):
   data=None if body is None else json.dumps(body).encode();r=urllib.request.Request(base+path,data=data,headers={'X-Atlas-Token':'owner-test-secret','Content-Type':'application/json'},method=method)
   try:
    with urllib.request.urlopen(r,timeout=10) as x:return x.status,json.load(x)
   except urllib.error.HTTPError as x:return x.code,json.load(x)
  def kill():
   try:os.killpg(proc.pid,signal.SIGKILL)
   except ProcessLookupError:pass
   proc.wait(timeout=10)
  try:
   start(True);wid='default'
   if mode=='protected':
    status,w=req('/api/workspaces',{'name':'Protected','protected':True,'password':'test-password'});assert status==201,(mode,status,w);wid=w['id']
   if 'external' in mode:
    disk=root/'disk';disk.mkdir();status,c=req('/api/workspaces/authorize',{'operation':'export','path':str(disk)});assert status==200
    protected=mode=='protected-external';status,w=req('/api/workspaces/create-external',{'capability':c['capability'],'name':'External','protected':protected,'password':'test-password'});assert status==201,(mode,status,w)
    status,w=req('/api/workspaces/attach',{'capability':w['capability'],'password':'test-password','test_protected':protected});assert status==200,(mode,status,w);wid=w['id']
   prefix='/api/w/'+wid;status,n=req(prefix+'/notes',{'title':'Title','text':'Old'});assert status==201,(mode,status,n);note=prefix+'/notes/'+n['id'];status,l=req(note+'/edit-lease',{'action':'acquire'});assert status==200,(mode,status,l)
   with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
    future=pool.submit(req,note+'/edit-commit',{'lease':l['lease'],'title':'Recovered','text':'Durable crash draft'})
    until=time.monotonic()+8
    while not (barrier/'ready').exists() and time.monotonic()<until:time.sleep(.01)
    assert (barrier/'ready').exists(),mode;kill()
    try:future.result(timeout=5)
    except Exception:pass
   start(False)
   if mode=='protected':status,r=req('/api/workspaces/'+wid+'/unlock',{'password':'test-password'});assert status==200,(mode,status,r)
   if 'external' in mode:
    status,c=req('/api/workspaces/remembered-attach',{'id':wid});assert status==200,(mode,status,c);status,r=req('/api/workspaces/attach',{'id':wid,'capability':c['capability'],'password':'test-password','test_protected':mode=='protected-external'});assert status==200,(mode,status,r)
   status,r=req(prefix+'/note-drafts/recover',{});assert status==200,(mode,status,r);status,r=req(note);assert status==200 and r['text']=='Durable crash draft',(mode,status,r)
  finally:
   if proc and proc.poll() is None:kill()
