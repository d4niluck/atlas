#!/usr/bin/env python3
"""An external board remains usable while its index check is stalled."""
import fcntl, argparse, concurrent.futures, json, os, pathlib, shutil, signal, subprocess, tempfile, time, threading
from urllib.request import Request, urlopen, install_opener, build_opener, ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--revision-barrier',action='store_true');p.add_argument('--export-during',action='store_true');p.add_argument('--crash-stage',action='store_true');p.add_argument('--crash-published',action='store_true');p.add_argument('--crash-promoted',action='store_true');p.add_argument('--staging-busy',action='store_true');p.add_argument('--force-compact',action='store_true');p.add_argument('--begin-barrier',action='store_true');p.add_argument('--generation-race',action='store_true');p.add_argument('--base-missing',action='store_true');p.add_argument('--initial',action='store_true');p.add_argument('--backend',required=True);p.add_argument('--resources',required=True);p.add_argument('--cancel',action='store_true');p.add_argument('--cancel-before-commit',action='store_true');p.add_argument('--cancel-inside-gate',action='store_true');a=p.parse_args();a.cancel=a.cancel or a.cancel_before_commit or a.cancel_inside_gate
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-ready-test-')); barrier=root/'barrier';barrier.mkdir();models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-model')
(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
mock=pathlib.Path(__file__).with_name('mock-portable-embedding-server.py');token='atlas-ready-fixture';env=os.environ.copy();env.update(NO_PROXY='*',DYLD_LIBRARY_PATH=a.resources,LD_LIBRARY_PATH=a.resources,ATLAS_TEST_READY_CANCELLED_MARK=str(barrier/'cancelled'),ATLAS_TEST_FAKE_ENCODER_LOG=str(root/'calls'))
env['ATLAS_TEST_INDEX_BEGIN_BARRIER' if a.begin_barrier else ('ATLAS_TEST_INDEX_PROMOTED_BARRIER' if a.crash_promoted else ('ATLAS_TEST_INDEX_GATE_BARRIER' if a.cancel_inside_gate else ('ATLAS_TEST_INDEX_PUBLISHED_BARRIER' if a.crash_published else ('ATLAS_TEST_INDEX_REVISION_BARRIER' if a.revision_barrier else ('ATLAS_TEST_INDEX_STAGE_BARRIER' if a.cancel_before_commit or a.crash_stage else 'ATLAS_TEST_INDEX_COMMIT_BARRIER')))))]=str(barrier)
if a.force_compact:env['ATLAS_TEST_FORCE_WORKER_COMPACT']=str(barrier)
proc=None
def req(path,data=None):
 q=Request(base+path,data=None if data is None else json.dumps(data).encode(),headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
 try:
  with urlopen(q,timeout=30) as response:return response.status,json.load(response)
 except HTTPError as error:return error.code,json.loads(error.read())
def ok(code,result):assert result[0]==code,result;return result[1]
def cap(op,path):return ok(200,req('/api/workspaces/authorize',{'operation':op,'path':str(path)}))['capability']
def search(wid,obj):
 deadline=time.monotonic()+90
 while time.monotonic()<deadline:
  code,data=req('/api/w/'+wid+'/search',{'mode':'semantic','query':'sample','object_ids':[obj]})
  if code==200:return data['results']
  assert code==503,(code,data);time.sleep(.1)
 raise AssertionError('search did not recover')
try:
 proc=subprocess.Popen([a.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env)
 proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit();base='http://127.0.0.1:'+port
 disk=root/'disk';disk.mkdir();made=ok(201,req('/api/workspaces/create-external',{'capability':cap('export',disk),'name':'Ready test','protected':False}));wid=ok(200,req('/api/workspaces/attach',{'capability':made['capability']}))['id'];external=next(disk.glob('Atlas-*'))
 if a.initial:(barrier/'enabled').touch()
 obj=ok(201,req('/api/w/'+wid+'/notes',{'title':'Original','text':'sample content'}))['id']
 if a.initial:changed=obj
 else:
  assert search(wid,obj);(barrier/'enabled').touch()
  changed=ok(201,req('/api/w/'+wid+'/notes',{'title':'Index target','text':'new semantic content'}))['id']
 deadline=time.monotonic()+90
 while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.05)
 assert (barrier/'ready').exists(),'index commit did not reach barrier'
 if a.force_compact:assert (barrier/'compacted').exists(),'forced compaction was not exercised before barrier'
 if a.initial and not a.crash_published:assert not (external/'.atlas-worker-index/current').exists(),'initial test unexpectedly had a current generation'
 if not a.initial and not a.crash_published:assert (external/'.atlas-worker-index/current').exists(),'delta test has no previous generation'
 if a.generation_race or a.base_missing:
  index=external/'.atlas-worker-index';current=index/'current';original=current.read_text().strip();manifest=json.loads((index/original/'overlay.json').read_text());oldbase=manifest['base_generation'];saved=root/'saved-base';shutil.move(index/oldbase,saved)
  if a.generation_race:
   newbase='gen-'+'e'*32+'-base';shutil.copytree(saved,index/newbase);current.write_text(newbase)
  (barrier/'go').touch();deadline=time.monotonic()+30
  if a.generation_race:
   while time.monotonic()<deadline:
    state=ok(200,req('/api/w/'+wid+'/semantic/status'))
    if state.get('available') and not state.get('indexing') and not state.get('checking') and state.get('message')=='':break
    assert state.get('message')!='Поиск по смыслу недоступен',state;time.sleep(.1)
   else:raise AssertionError(('new generation did not recover',state))
   assert search(wid,obj);assert search(wid,changed);print('PASS superseded base retries automatically')
  else:
   while time.monotonic()<deadline:
    state=ok(200,req('/api/w/'+wid+'/semantic/status'))
    if state.get('message')=='Поиск по смыслу недоступен':break
    time.sleep(.1)
   else:raise AssertionError(('missing active base was not reported',state))
   print('PASS missing active base is reported')
 elif a.staging_busy:
  lockpath=external/'.atlas-worker-index/.staging.lock'
  with open(lockpath,'a+b') as lock:
   fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);(barrier/'go').touch();time.sleep(4)
   state=ok(200,req('/api/w/'+wid+'/semantic/status'));assert state.get('message')!='Поиск по смыслу недоступен',state
   fcntl.flock(lock,fcntl.LOCK_UN)
  deadline=time.monotonic()+90
  while time.monotonic()<deadline:
   state=ok(200,req('/api/w/'+wid+'/semantic/status'))
   if state.get('available') and not state.get('indexing') and not state.get('checking') and state.get('message')=='':break
   time.sleep(.1)
  else:raise AssertionError(('index did not automatically recover',state))
  assert search(wid,obj);assert search(wid,changed);print('PASS busy staging retries without permanent failure')
 elif a.crash_stage or a.crash_published or a.crash_promoted:
  os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=10);env.pop('ATLAS_TEST_INDEX_STAGE_BARRIER',None);env.pop('ATLAS_TEST_INDEX_PUBLISHED_BARRIER',None);env.pop('ATLAS_TEST_INDEX_PROMOTED_BARRIER',None)
  proc=subprocess.Popen([a.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env)
  proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit();base='http://127.0.0.1:'+port
  wid=ok(200,req('/api/workspaces/attach',{'capability':cap('attach',external),'id':wid}))['id'];assert search(wid,obj);assert search(wid,changed)
  print('PASS interrupted staged index recovered')
 elif a.export_during:
  destination=root/'export';destination.mkdir();capability=cap('export',destination)
  with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
   exporting=pool.submit(req,'/api/workspaces/export',{'capability':capability,'ids':[wid]})
   time.sleep(.2);assert not exporting.done(),'export completed while index writer was stalled'
   (barrier/'go').touch();response=exporting.result(timeout=30);assert response[0]==200,response
  print('PASS export waits for staged index cancellation')
 elif a.cancel:
  current=external/'.atlas-worker-index/current';before=current.read_text() if current.exists() else None
  if a.cancel_inside_gate:
   with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
    cancelling=pool.submit(req,'/api/w/'+wid+'/semantic/cancel',{})
    time.sleep(.2);assert not cancelling.done(),'cancel returned while commit gate held'
    (barrier/'go').touch();ok(200,cancelling.result(timeout=5))
   before=current.read_text() if current.exists() else None
  else:ok(200,req('/api/w/'+wid+'/semantic/cancel',{}));(barrier/'go').touch()
  time.sleep(.5);assert (current.read_text() if current.exists() else None)==before,'cancelled commit published index'
  deadline=time.monotonic()+20
  while time.monotonic()<deadline:
   state=ok(200,req('/api/w/'+wid+'/semantic/status'))
   if state.get('cancelled'):break
   time.sleep(.05)
  else:raise AssertionError(('cancel did not finish',state))
  ok(202,req('/api/w/'+wid+'/semantic/rebuild',{}));assert search(wid,obj);assert search(wid,changed)
  print('PASS cancelled index operation did not publish and search recovered')
 else:
  with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
   read=pool.submit(req,'/api/w/'+wid+'/board')
   write=pool.submit(req,'/api/w/'+wid+'/notes',{'title':'During commit','text':'concurrent text'})
   assert read.result(timeout=2)[0]==200,'board read blocked during index commit'
   response=write.result(timeout=2);assert response[0]==201,response;new=response[1]['id']
  (barrier/'go').touch()
  assert search(wid,obj);assert search(wid,new)
  if a.force_compact:assert (barrier/'compacted').exists(),'forced compaction was not exercised'
 if not a.cancel and not a.export_during and not a.crash_stage and not a.crash_published and not a.crash_promoted and not a.staging_busy and not a.generation_race and not a.base_missing:print('PASS external board read/write during background index commit')
finally:
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
 shutil.rmtree(root)
