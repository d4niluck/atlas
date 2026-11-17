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
 obj=ok(201,req('/api/w/'+wid+'/notes',{'title':'A','text':'initial text'}))['id'];assert search(wid,obj)
 ok(200,req('/api/w/'+wid+'/notes/'+obj,{'title':'A','text':marker},'PUT'));assert search(wid,obj);before=count();assert before==1,before
 ok(200,req('/api/w/'+wid+'/objects/delete-many',{'ids':[obj]}));deadline=time.monotonic()+120
 while time.monotonic()<deadline:
  status=ok(200,req('/api/w/'+wid+'/semantic/status'))
  if not status.get('indexing') and status.get('available'):break
  time.sleep(.1)
 else:raise AssertionError(('deletion indexing failed',status))
 ok(202,req('/api/w/'+wid+'/semantic/rebuild',{'force':True}));deadline=time.monotonic()+120
 while time.monotonic()<deadline:
  status=ok(200,req('/api/w/'+wid+'/semantic/status'))
  if not status.get('indexing') and status.get('available'):break
  time.sleep(.1)
 else:raise AssertionError(('rebuild failed',status))
 assert count()==before,('hidden vector encoded during forced rebuild',before,count())
 ok(200,req('/api/w/'+wid+'/trash/restore-many',{'ids':[obj]}));assert search(wid,obj);assert count()==before,('hidden delta vector lost',before,count())
 print('PASS hidden delta vector retained without re-encoding',flush=True)

finally:
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
 shutil.rmtree(root)
