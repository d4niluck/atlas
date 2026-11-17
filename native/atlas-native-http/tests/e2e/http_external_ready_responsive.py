#!/usr/bin/env python3
"""An external board remains usable while its index check is stalled."""
import argparse, concurrent.futures, json, os, pathlib, shutil, signal, subprocess, tempfile, time, threading, uuid
from urllib.request import Request, urlopen, install_opener, build_opener, ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',required=True);p.add_argument('--cancel',action='store_true');p.add_argument('--cancel-before-commit',action='store_true');p.add_argument('--cancel-inside-gate',action='store_true');p.add_argument('--generation-swap',action='store_true');p.add_argument('--generation-missing',action='store_true');a=p.parse_args();a.cancel=a.cancel or a.cancel_before_commit or a.cancel_inside_gate
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-ready-test-')); barrier=root/'barrier';barrier.mkdir();models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-model')
(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
mock=pathlib.Path(__file__).with_name('mock-portable-embedding-server.py');token='atlas-ready-fixture';env=os.environ.copy();env.update(NO_PROXY='*',DYLD_LIBRARY_PATH=a.resources,LD_LIBRARY_PATH=a.resources,ATLAS_TEST_READY_CANCELLED_MARK=str(barrier/'cancelled'),ATLAS_TEST_FAKE_ENCODER_LOG=str(root/'calls'))
if a.generation_swap or a.generation_missing:
 env['ATLAS_TEST_READY_GENERATION_BARRIER']=str(barrier);env['ATLAS_TEST_READY_GENERATION_RETRY']=str(barrier/'retry')
elif a.cancel_before_commit:env['ATLAS_TEST_READY_COMMIT_BARRIER']=str(barrier)
elif a.cancel_inside_gate:env['ATLAS_TEST_READY_GATE_BARRIER']=str(barrier)
else:env['ATLAS_TEST_READY_COMPACTION_BARRIER']=str(barrier)
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
 obj=ok(201,req('/api/w/'+wid+'/notes',{'title':'Original','text':'sample content'}))['id'];assert search(wid,obj)
 ok(200,req('/api/workspaces/'+wid+'/close',{}));(barrier/'enabled').touch();wid=ok(200,req('/api/workspaces/attach',{'capability':cap('attach',external),'id':wid}))['id']
 req('/api/w/'+wid+'/search',{'mode':'semantic','query':'sample','object_ids':[obj]})
 deadline=time.monotonic()+20
 while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.05)
 assert (barrier/'ready').exists(),'ready check did not reach barrier'
 if a.generation_swap or a.generation_missing:
  index=external/'.atlas-worker-index';old=(index/'current').read_text()
  if a.generation_swap:
   new='gen-'+uuid.uuid4().hex;shutil.copytree(index/old,index/new)
   (index/'current.tmp').write_text(new);os.replace(index/'current.tmp',index/'current')
  shutil.rmtree(index/old)
 if a.generation_missing:
  (barrier/'go').touch();deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   state=ok(200,req('/api/w/'+wid+'/semantic/status'))
   if 'недоступен' in state.get('message','').lower() or 'не удалось' in state.get('message','').lower():break
   time.sleep(.05)
  else:raise AssertionError(('missing generation was not reported',state))
  assert not (barrier/'retry').exists(),'missing generation incorrectly treated as concurrency'
  print('PASS missing current generation remains an error');raise SystemExit(0)
 if a.cancel:
  current=external/'.atlas-worker-index/current';generation=current.read_text() if current.exists() else None
  if a.cancel_inside_gate:
   with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
    cancellation=pool.submit(req,'/api/w/'+wid+'/semantic/cancel',{})
    time.sleep(.15);assert not cancellation.done(),'cancel returned while publication gate was held'
    (barrier/'go').touch();ok(200,cancellation.result(timeout=5))
  else:ok(200,req('/api/w/'+wid+'/semantic/cancel',{}))
  deadline=time.monotonic()+5
  while not (barrier/'cancelled').exists() and time.monotonic()<deadline:time.sleep(.02)
  assert (barrier/'cancelled').exists(),'cancel did not stop background index preparation'
  assert (current.read_text() if current.exists() else None)==generation,'index was published after cancellation'
 with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
  read=pool.submit(req,'/api/w/'+wid+'/board');write=pool.submit(req,'/api/w/'+wid+'/notes',{'title':'During check','text':'new content'})
  assert read.result(timeout=2)[0]==200,'board read blocked';response=write.result(timeout=2);assert response[0]==201,response;new=response[1]['id']
 (barrier/'go').touch()
 if a.cancel:
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   state=ok(200,req('/api/w/'+wid+'/semantic/status'))
   if state.get('cancelled'):break
   time.sleep(.05)
  else:raise AssertionError(('cancel did not finish',state))
  ok(202,req('/api/w/'+wid+'/semantic/rebuild',{}))
 assert search(wid,obj);assert search(wid,new)
 if a.generation_swap:assert (barrier/'retry').exists(),'missing-generation retry did not occur'
 print('PASS external board read/write during background index check')
finally:
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
 shutil.rmtree(root)
