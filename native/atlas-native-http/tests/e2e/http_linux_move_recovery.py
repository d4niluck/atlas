#!/usr/bin/env python3
"""Kill a test-hook backend at each transfer checkpoint and inspect recovery.
Usage: crash_recovery.py BACKEND [ordinary|protected] [journal|registered|renamed|committed|all]
ATLAS_TEST_FLASH_ROOT optionally places ONLY destination copies in a newly created subdirectory there.
Every workspace and artifact is synthetic; application data and existing flash files are never changed.
"""
import json, os, pathlib, signal, subprocess, sys, tempfile, threading, time
from urllib.request import Request, build_opener, ProxyHandler
from urllib.error import HTTPError, URLError
opener=build_opener(ProxyHandler({}))
binary=os.path.abspath(sys.argv[1]);kind=sys.argv[2] if len(sys.argv)>2 else 'ordinary';which=sys.argv[3] if len(sys.argv)>3 else 'all'
assert kind in ('ordinary','protected');phases=['journal','registered','renamed','committed'] if which=='all' else [which];assert set(phases)<={'journal','registered','renamed','committed'}
token='atlas-test-token';password='isolated-only-pass';payload=b'physical-transfer-fixture-903\x00\xff'*64

def run(phase):
 with tempfile.TemporaryDirectory(prefix='atlas-crash-test-') as temp:
  root=pathlib.Path(temp);data=root/'data';barrier=root/'barrier';barrier.mkdir();flash=os.environ.get('ATLAS_TEST_FLASH_ROOT')
  with tempfile.TemporaryDirectory(prefix='atlas-crash-disk-',dir=flash or temp) as diskname:
   disk=pathlib.Path(diskname);sentry=disk/'unrelated';sentry.write_bytes(b'unchanged');proc=None;url=''
   def start(hooks):
    nonlocal proc,url
    env=os.environ.copy()
    if hooks:env['ATLAS_TEST_MOVE_BARRIER']=str(barrier)
    else:env.pop('ATLAS_TEST_MOVE_BARRIER',None)
    proc=subprocess.Popen([binary,'--root',str(data),'--port','0','--token-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env)
    proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip()
    assert port.isdigit(),('backend start failed',port,proc.poll());url='http://127.0.0.1:'+port
   def stop(force=False):
    nonlocal proc
    if proc and proc.poll() is None:
     os.killpg(proc.pid,signal.SIGKILL if force else signal.SIGTERM)
     try:proc.wait(timeout=15)
     except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
    if proc:
     if proc.stdin:proc.stdin.close()
     if proc.stdout:proc.stdout.close()
    proc=None
   def req(path,value=None,method=None,raw=None):
    body=raw if raw is not None else None if value is None else json.dumps(value).encode()
    q=Request(url+path,data=body,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream' if raw is not None else 'application/json'})
    try:
     with opener.open(q,timeout=90) as r:
      return (r.status,r.read()) if path.endswith('/content') else (r.status,json.load(r))
    except HTTPError as e:
     with e:return e.code,json.load(e)
   def expect(code,r):assert r[0]==code,(code,r);return r[1]
   def verify(wid,nid,fid):
    assert expect(200,req('/api/w/'+wid+'/notes/'+nid))['text']=='safe synthetic note 903'
    assert expect(200,req('/api/w/'+wid+'/files/'+fid+'/content'))==payload
   thread=None
   try:
    for name in ['journal','registered','marker','renamed','committed']:
     if name!=phase:(barrier/(name+'.resume')).write_text('resume')
    start(True)
    value={'name':'isolated crash test','protected':kind=='protected'}
    if kind=='protected':value['password']=password
    wid=expect(201,req('/api/workspaces',value))['id'];nid=expect(201,req('/api/w/'+wid+'/notes',{'title':'fixture','text':'safe synthetic note 903'}))['id']
    file=expect(200,req('/api/w/'+wid+'/files?filename=fixture.bin',raw=payload));fid=file['id']
    if kind=='protected':expect(200,req('/api/workspaces/'+wid+'/close',{}))
    cap=expect(200,req('/api/workspaces/authorize',{'operation':'export','path':str(disk)}))['capability'];body={'capability':cap,'ids':[wid],'move':True}
    if kind=='protected':body['passwords']={wid:password}
    def transfer():
     try:req('/api/workspaces/export',body)
     except (OSError,URLError):pass
    thread=threading.Thread(target=transfer,daemon=True);thread.start()
    deadline=time.monotonic()+90
    while not (barrier/(phase+'.ready')).exists():
     assert proc.poll() is None,('backend stopped before barrier',phase,proc.returncode)
     assert thread.is_alive(),('transfer returned before barrier',phase)
     assert time.monotonic()<deadline,('timeout waiting for barrier',phase)
     time.sleep(.1)
    stop(True);thread.join(timeout=5);assert not thread.is_alive(),'request thread did not exit'
    start(False)
    assert sentry.read_bytes()==b'unchanged'
    if phase!='committed':
     if kind=='protected':expect(200,req('/api/workspaces/'+wid+'/unlock',{'password':password}))
     verify(wid,nid,fid)
    else:
     targets=list(disk.glob('Atlas-*/workspace-1'));assert len(targets)==1,targets
     cap=expect(200,req('/api/workspaces/authorize',{'operation':'attach','path':str(targets[0])}))['capability']
     listed=expect(200,req('/api/workspaces'))['workspaces'];matches=[x for x in listed if x.get('external') and x.get('name')=='isolated crash test'];assert len(matches)==1,matches
     attach={'capability':cap,'id':matches[0]['id']}
     if kind=='protected':attach.update(password=password,test_protected=True)
     # A killed child can take a moment to release its OS file lock.
     deadline=time.monotonic()+10
     while True:
      status,value=req('/api/workspaces/attach',attach)
      if status==200:break
      if time.monotonic()>=deadline:
       diagnostic=subprocess.run([binary,'--protected-workspace-worker',str(targets[0])],cwd=str(targets[0]),input='{"action":"ping"}\n',capture_output=True,text=True,timeout=15)
       raise AssertionError(('attach after crash',status,value,'worker',diagnostic.returncode,diagnostic.stdout,diagnostic.stderr))
      time.sleep(.25)
      attach['capability']=expect(200,req('/api/workspaces/authorize',{'operation':'attach','path':str(targets[0])}))['capability']
     verify(value['id'],nid,fid)
     # Current recovery keeps the source tomb and journal pending for committed work.
     assert list(data.glob('.atlas-move-*.json')),'committed journal missing'
     assert any(p.is_dir() for p in data.glob('.atlas-move-*')),'committed source tomb missing'
    print('PASS crash recovery:',kind,phase,flush=True)
   finally:
    stop(True)
    if thread:thread.join(timeout=5)
for phase in phases:run(phase)
