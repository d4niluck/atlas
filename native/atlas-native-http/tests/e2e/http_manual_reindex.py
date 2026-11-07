#!/usr/bin/env python3
"""Verify forced rebuild re-encodes a workspace; only synthetic data are used."""
import argparse,json,os,pathlib,shutil,signal,subprocess,tempfile,time
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',default='');p.add_argument('--protected',action='store_true');p.add_argument('--external',action='store_true');p.add_argument('--image-failure',action='store_true');args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-force-rebuild-')); models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-model'); log=root/'encoder.jsonl'; token='test-token'; marker='REBUILD_TEST_621576'; password='test-password';proc=None
settings={'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':args.image_failure,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'};(models/'settings.json').write_text(json.dumps(settings));(models/'embeddinggemma-2-mmproj-q8.gguf').write_bytes(b'fake-image-model')
mock=root/'mock-encoder.py';mock_source=pathlib.Path(__file__).with_name('mock-portable-embedding-server.py').read_text();mock_source=mock_source.replace('import sys,socketserver,http.server,json,hashlib,os','import sys,socketserver,http.server,json,hashlib,os,time,pathlib');mock_source=mock_source.replace('  h=hashlib.sha256(text.encode()).digest();',"  barrier=pathlib.Path(os.environ['ATLAS_REBUILD_BARRIER']);\n  if barrier.with_suffix('.pause').exists() and 'REBUILD_TEST_621576' in text:\n   barrier.with_suffix('.ready').write_text('ready'); deadline=time.monotonic()+30\n   while not barrier.with_suffix('.go').exists() and time.monotonic()<deadline:time.sleep(.02)\n  h=hashlib.sha256(text.encode()).digest();");mock.write_text(mock_source);mock.chmod(0o700)
env=os.environ.copy();env.update(NO_PROXY='*',ATLAS_TEST_FAKE_ENCODER_LOG=str(log),ATLAS_REBUILD_BARRIER=str(root/'barrier'),DYLD_LIBRARY_PATH=args.resources,LD_LIBRARY_PATH=args.resources)
def req(path,body=None,method=None):
 data=None if body is None else json.dumps(body,ensure_ascii=False).encode();q=Request(base+path,data=data,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
 try:
  with urlopen(q,timeout=30) as r:return r.status,json.load(r)
 except HTTPError as e:
  try:return e.code,json.loads(e.read())
  except Exception:return e.code,{}
def ok(code,response):assert response[0]==code,response;return response[1]
def count():
 if not log.exists():return 0
 return sum(marker in x.get('input','') and x['input'].startswith('title: none | text: ') for x in map(json.loads,log.read_text().splitlines()))
def search(wid,obj):
 end=time.monotonic()+120
 while time.monotonic()<end:
  code,data=req('/api/w/'+wid+'/search',{'mode':'semantic','query':'sample query','object_ids':[obj]})
  if code==200:return data['results']
  assert code in (409,503),(code,data);time.sleep(.1)
 raise AssertionError(('search timeout',data))
def cap(op,path):return ok(200,req('/api/workspaces/authorize',{'operation':op,'path':str(path)}))['capability']
try:
 proc=subprocess.Popen([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=None,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('backend failed',port);base='http://127.0.0.1:'+port
 wid='default'
 if args.external:
  disk=root/'disk';disk.mkdir();made=ok(201,req('/api/workspaces/create-external',{'capability':cap('export',disk),'name':'Test','protected':args.protected,'password':password}));attach={'capability':made['capability']}
  if args.protected:attach.update(password=password,test_protected=True)
  wid=ok(200,req('/api/workspaces/attach',attach))['id']
 elif args.protected:wid=ok(201,req('/api/workspaces',{'name':'Test','protected':True,'password':password}))['id']
 obj=ok(201,req('/api/w/'+wid+'/notes',{'title':'A','text':marker}))['id'];assert search(wid,obj);before=count();assert before==1,before
 # A damaged, unrelated worker index must not block an intact native index.
 if not args.external and not args.protected:
  worker=root/'data/.default/.atlas-worker-index';worker.mkdir();(worker/'current').write_text('gen-broken')
 ok(202,req('/api/w/'+wid+'/semantic/rebuild',{'force':False}));end=time.monotonic()+120
 while time.monotonic()<end:
  status=ok(200,req('/api/w/'+wid+'/semantic/status'))
  if not status.get('indexing'):break
  time.sleep(.1)
 else:raise AssertionError(('resume did not complete',status))
 assert search(wid,obj);assert count()==before,('normal resume unexpectedly re-encoded',before,count());print('PASS resume retains vectors with intact index',flush=True)
 ok(202,req('/api/w/'+wid+'/semantic/rebuild',{'force':True}));end=time.monotonic()+120
 while time.monotonic()<end:
  status=ok(200,req('/api/w/'+wid+'/semantic/status'))
  if not status.get('indexing') and count()>before:break
  time.sleep(.1)
 else:raise AssertionError(('rebuild did not complete',status,count()))
 assert count()==before+1,('expected re-encoding',before,count());assert search(wid,obj);print('PASS forced rebuild re-encoded and remains searchable',flush=True)
 current=None
 if not args.protected:
  if args.external:current=next((root/'disk').glob('Atlas-*'))/'.atlas-worker-index/current'
  else:current=root/'data/.default/.atlas-semantic-native/current'
 old_current=current.read_bytes() if current else None
 (root/'barrier.pause').write_text('pause');ok(202,req('/api/w/'+wid+'/semantic/rebuild',{'force':True}));deadline=time.monotonic()+60
 while not (root/'barrier.ready').exists() and time.monotonic()<deadline:time.sleep(.05)
 assert (root/'barrier.ready').exists(),'encoder did not reach cancellation barrier'
 ok(200,req('/api/w/'+wid+'/semantic/cancel',{}));(root/'barrier.go').write_text('go');deadline=time.monotonic()+60
 while time.monotonic()<deadline:
  status=ok(200,req('/api/w/'+wid+'/semantic/status'))
  if status.get('cancelled') and not status.get('indexing'):break
  time.sleep(.05)
 else:raise AssertionError(('cancellation did not finish',status))
 if current:assert current.read_bytes()==old_current,'cancelled rebuild replaced existing index'
 (root/'barrier.pause').unlink();before_resume=count();ok(202,req('/api/w/'+wid+'/semantic/rebuild',{'force':False}));assert search(wid,obj);assert count()==before_resume,('resume recalculated after cancel',before_resume,count());print('PASS cancelled rebuild preserves index and normal resume',flush=True)
 if args.image_failure:
  q=Request(base+'/api/w/'+wid+'/files?filename=invalid.png',data=b'fake png',method='POST',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
  with urlopen(q,timeout=30) as response:assert response.status==200
  assert search(wid,obj);old_current=current.read_bytes();ok(202,req('/api/w/'+wid+'/semantic/rebuild',{'force':True}));deadline=time.monotonic()+60
  while time.monotonic()<deadline:
   status=ok(200,req('/api/w/'+wid+'/semantic/status'))
   if not status.get('indexing') and not status.get('available'):break
   time.sleep(.05)
  else:raise AssertionError(('image failure not reported',status))
  assert current.read_bytes()==old_current,'failed image encoding replaced prior index';print('PASS image failure preserves old current',flush=True)
 cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,semantic_enabled=False),'PUT'));code,data=req('/api/w/'+wid+'/semantic/rebuild',{'force':True});assert code!=202,(code,data);print('PASS disabled semantic search rejects rebuild',flush=True)
finally:
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
 shutil.rmtree(root)
