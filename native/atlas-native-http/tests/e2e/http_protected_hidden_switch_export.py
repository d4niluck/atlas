#!/usr/bin/env python3
"""Black-box protected-index contract with local models and isolated data only."""
import argparse,json,os,pathlib,signal,subprocess,tempfile,time,shutil
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',default=None);args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-index-protected-contract-'));models=root/'models';models.mkdir()
(models/'embeddinggemma-2-q8.gguf').write_bytes(b'q8-fake-model')
(models/'embeddinggemma-2.gguf').write_bytes(b'bf16-fake-model')
(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'metal','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
res=args.resources;token='isolated-index-contract';env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_TEST_FAKE_ENCODER_LOG':str(root/'calls.jsonl')})

if res:
 env['DYLD_LIBRARY_PATH']=res+os.pathsep+env.get('DYLD_LIBRARY_PATH','')
 env['LD_LIBRARY_PATH']=res+os.pathsep+env.get('LD_LIBRARY_PATH','')
proc=None;base='';marker='ATLAS_PROTECTED_INDEX_CONFIDENTIAL_4f35a978'
def start():
 global proc,base
 proc=subprocess.Popen([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).resolve().with_name('mock-embedding-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env)
 proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('start failed',port);base='http://127.0.0.1:'+port

def stop():
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=10)

def request(path,data=None,method=None):
 payload=None if data is None else json.dumps(data,ensure_ascii=False).encode();q=Request(base+path,data=payload,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
 try:
  with urlopen(q,timeout=120) as r:return r.status,json.load(r)
 except HTTPError as e:
  try:return e.code,json.loads(e.read())
  except Exception:return e.code,{}
def ok(expected,result):assert result[0]==expected,result;return result[1]
def search(wid,query,ids):
 deadline=time.monotonic()+120
 while time.monotonic()<deadline:
  code,result=request('/api/w/'+wid+'/search',{'mode':'semantic','query':query,'object_ids':ids})
  if code==200:return result['results']
  assert code==503,(code,result)
  time.sleep(.25)
 raise AssertionError(('semantic search timeout',result))
def no_plaintext():
 # Inspect only paths created by this test, including sidecar journal files.
 for path in (root/'data').rglob('*'):
  if path.is_symlink() or not path.is_file():continue
  assert marker.encode() not in path.read_bytes(),f'confidential source text leaked to {path.name}'
import hashlib
password='test-password-01';marker_a='ATLAS_EXPIRED_A_3f8d572';marker_b='ATLAS_KEEP_B_2de451c';log=root/'calls.jsonl'
def encoded(marker_text):
 return sum(marker_text in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
def unlock(wid):ok(200,request('/api/workspaces/'+wid+'/unlock',{'password':password}))
def wait_error(wid):
 deadline=time.monotonic()+30
 while time.monotonic()<deadline:
  code,status=request('/api/w/'+wid+'/semantic/status');assert code==200,(code,status)
  if status.get('message') and not status.get('indexing'):return status
  time.sleep(.1)
 raise AssertionError(('corrupt index not rejected',status))
def cap(op,path):return ok(200,request('/api/workspaces/authorize',{'operation':op,'path':str(path)}))['capability']
def switch(variant):cfg=ok(200,request('/api/settings/app'));return ok(200,request('/api/settings/app',dict(cfg,embedding_variant=variant),'PUT'))
try:
 start();wid=ok(201,request('/api/workspaces',{'name':'Protected switch/export','protected':True,'password':password}))['id'];a=ok(201,request('/api/w/'+wid+'/notes',{'title':'A','text':'old protected'}))['id'];b=ok(201,request('/api/w/'+wid+'/notes',{'title':'B','text':'other protected'}))['id'];assert search(wid,'other',[b]);ok(200,request('/api/w/'+wid+'/notes/'+a,{'title':'A','text':marker_a},'PUT'));assert search(wid,'A',[a]);assert encoded(marker_a)==1
 stop()
 start();unlock(wid);ok(200,request('/api/w/'+wid+'/objects/delete-many',{'ids':[a]}));assert search(wid,'B',[b]);assert not search(wid,'A',[a]);switch('bf16');assert search(wid,'B',[b]);assert encoded(marker_a)==2,('hidden not encoded in new model',encoded(marker_a));stop()
 start();unlock(wid);assert not search(wid,'A',[a]);ok(200,request('/api/w/'+wid+'/trash/restore-many',{'ids':[a]}));assert search(wid,'A',[a]);assert encoded(marker_a)==2
 ok(200,request('/api/w/'+wid+'/objects/delete-many',{'ids':[a]}));assert search(wid,'B',[b]);switch('q8');assert search(wid,'B',[b]);assert encoded(marker_a)==3,('hidden q8 reencode missing',encoded(marker_a));stop()
 print('PASS protected model switches with hidden and restart',flush=True)
 start();unlock(wid);dest=root/'exports';dest.mkdir();before=encoded(marker_a);ok(200,request('/api/workspaces/'+wid+'/close',{}));exported=ok(200,request('/api/workspaces/export',{'capability':cap('export',dest),'ids':[wid],'passwords':{wid:password}}));folder=pathlib.Path(exported.get('path') or dest/exported['folder']);assert folder.is_dir();imported=ok(201,request('/api/workspaces/import',{'capability':cap('import',folder)}))['workspaces'];assert len(imported)==1;copy=imported[0]['id'];unlock(copy);assert search(copy,'B',[b]);assert not search(copy,'A',[a]);ok(200,request('/api/w/'+copy+'/trash/restore-many',{'ids':[a]}));assert search(copy,'A',[a]);assert encoded(marker_a)==before,('imported hidden reencoded',before,encoded(marker_a));print('PASS protected export/import hidden without reencode',root,flush=True)
finally:
 stop()
 shutil.rmtree(root)
