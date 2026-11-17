#!/usr/bin/env python3
"""Expired local trash is automatically purged without losing retained vectors."""
import argparse,json,os,pathlib,signal,sqlite3,subprocess,tempfile,time,shutil,threading
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',default=None);p.add_argument('--revision-barrier',action='store_true');args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-trash-retention-'));models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file')
(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
log=root/'calls.jsonl';res=args.resources;token='isolated-trash-contract';env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_TEST_FAKE_ENCODER_LOG':str(log)})

if res:
 env['DYLD_LIBRARY_PATH']=res+os.pathsep+env.get('DYLD_LIBRARY_PATH','')
 env['LD_LIBRARY_PATH']=res+os.pathsep+env.get('LD_LIBRARY_PATH','')
proc=None;base='';marker='ATLAS_EDITED_DELTA_SENTINEL_a139c5'
def start():
 global proc,base
 proc=subprocess.Popen([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).resolve().with_name('mock-embedding-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=None,text=True,start_new_session=True,env=env)
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
try:
 barrier=root/'pin-barrier';barrier.mkdir();env['ATLAS_TEST_TRASH_PIN_BARRIER']=str(barrier)
 start()
 original=root/'data/.default'
 a=ok(201,request('/api/w/default/notes',{'title':'Original','text':'original'}))
 result=[]
 def cleanup():
  try:result.append(request('/api/w/default/trash'))
  except Exception as e:result.append(e)
 t=threading.Thread(target=cleanup,daemon=True);t.start()
 deadline=time.monotonic()+30
 while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.05)
 assert (barrier/'ready').exists(),'cleanup did not pin directory'
 moved=root/'data/original-moved';original.rename(moved)
 # A replacement must not be inspected or modified by the pinned cleanup.
 original.mkdir();(original/'sentinel').write_text('untouched')
 (barrier/'go').touch();t.join(timeout=30);assert not t.is_alive(),'cleanup stuck'
 assert result and isinstance(result[0],tuple) and result[0][0]==409,result
 assert sorted(p.name for p in original.iterdir())==['sentinel']
 assert (original/'sentinel').read_text()=='untouched'
 print('PASS cleanup remains pinned across workspace replacement',flush=True)
finally:
 stop();shutil.rmtree(root)
