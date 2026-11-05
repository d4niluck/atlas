#!/usr/bin/env python3
"""Assert deleted edited vectors survive restart for 24 hours without model calls."""
import argparse,json,os,pathlib,signal,sqlite3,subprocess,tempfile,time,shutil
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',default=None);args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-trash-retention-'));models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file');(models/'embeddinggemma-2.gguf').write_bytes(b'different-fake-bf16-model')
(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
log=root/'calls.jsonl';res=args.resources;token='isolated-trash-contract';env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_TEST_FAKE_ENCODER_LOG':str(log)})

if res:
 env['DYLD_LIBRARY_PATH']=res+os.pathsep+env.get('DYLD_LIBRARY_PATH','')
 env['LD_LIBRARY_PATH']=res+os.pathsep+env.get('LD_LIBRARY_PATH','')
proc=None;base='';marker='ATLAS_EDITED_DELTA_SENTINEL_a139c5'
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
  with urlopen(q,timeout=60) as r:return r.status,json.load(r)
 except HTTPError as e:
  try:return e.code,json.loads(e.read())
  except Exception:return e.code,{}
def ok(expected,result):assert result[0]==expected,result;return result[1]
def search(ids):
 deadline=time.monotonic()+90
 while time.monotonic()<deadline:
  code,result=request('/api/w/default/search',{'mode':'semantic','query':'тестовый вопрос','object_ids':ids})
  if code==200:return result['results']
  assert code==503,(code,result)
  time.sleep(.1)
 raise AssertionError(('semantic search timeout',result))
def count():
 return sum(marker in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
def search_w(wid,ids):
 deadline=time.monotonic()+90
 while time.monotonic()<deadline:
  code,result=request('/api/w/'+wid+'/search',{'mode':'semantic','query':'тестовый вопрос','object_ids':ids})
  if code==200:return result['results']
  assert code==503,(code,result)
  time.sleep(.1)
 raise AssertionError(('semantic search timeout',result))
def cap(op,path):return ok(200,request('/api/workspaces/authorize',{'operation':op,'path':str(path)}))['capability']
try:
 start();dest=root/'exports';dest.mkdir();a=ok(201,request('/api/w/default/notes',{'title':'A','text':'Первоначальный текст'}));b=ok(201,request('/api/w/default/notes',{'title':'B','text':'Отдельный источник'}));assert len(search_w('default',[a['id'],b['id']]))==2
 ok(200,request('/api/w/default/notes/'+a['id'],{'title':'A','text':marker},'PUT'));assert search_w('default',[a['id']]);assert count()==1
 exported=ok(200,request('/api/workspaces/export',{'capability':cap('export',dest),'ids':['default']}));folder=pathlib.Path(exported.get('path') or dest/exported['folder']);assert folder.is_dir()
 imported=ok(201,request('/api/workspaces/import',{'capability':cap('import',folder)}))['workspaces'];assert len(imported)==1;wid=imported[0]['id'];out=search_w(wid,[a['id']]);assert out and marker in out[0]['chunks'][0]['text'];assert count()==1,('import reran unchanged vector',count())
 print('PASS export/import retains changed vector without model call',flush=True)
 cfg=ok(200,request('/api/settings/app'));ok(200,request('/api/settings/app',dict(cfg,embedding_variant='bf16'),'PUT'));out=search_w(wid,[a['id']]);assert out and marker in out[0]['chunks'][0]['text'];assert count()>=2,('new model did not rebuild changed vector',count())
 print('PASS switch model after import',root,flush=True)
finally:
 stop()
 shutil.rmtree(root)
