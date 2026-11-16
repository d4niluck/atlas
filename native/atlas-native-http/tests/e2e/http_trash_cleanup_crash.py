#!/usr/bin/env python3
"""Expired local trash is automatically purged without losing retained vectors."""
import argparse,json,os,pathlib,signal,sqlite3,subprocess,tempfile,time,shutil,threading
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',default=None);args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-trash-retention-'));models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file')
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
try:
 start();a=ok(201,request('/api/w/default/notes',{'title':'A','text':'Первоначальная запись.'}));b=ok(201,request('/api/w/default/notes',{'title':'B','text':'Другая запись.'}));assert len(search([a['id'],b['id']]))==2
 ok(200,request('/api/w/default/notes/'+a['id'],{'title':'A','text':marker},'PUT'));assert search([a['id']]);assert count()==1,('first edited encode count',count())
 ok(200,request('/api/w/default/objects/delete-many',{'ids':[a['id']]}));assert search([a['id']])==[]
 stop();start();assert search([a['id']])==[]
 ok(200,request('/api/w/default/trash/restore-many',{'ids':[a['id']]}));assert search([a['id']]);assert count()==1,('undo recalculated edited vector',count())
 print('PASS restore after restart: no model call',flush=True)
 ok(200,request('/api/w/default/objects/delete-many',{'ids':[a['id']]}));assert search([a['id']])==[]
 fresh=ok(201,request('/api/w/default/notes',{'title':'Fresh','text':'RECENT_TRASH'}));assert search([fresh['id']])
 ok(200,request('/api/w/default/objects/delete-many',{'ids':[fresh['id']]}))
 db=root/'data/.default/atlas.sqlite3'
 with sqlite3.connect(db) as c:c.execute('UPDATE trash SET deleted_at=0 WHERE id=?',(a['id'],));c.commit()
 # An open workspace is cleaned by its periodic trash check.
 barrier=root/'purge-barrier';barrier.mkdir();env['ATLAS_TEST_NATIVE_PURGE_BARRIER']=str(barrier)
 stop();start()
 def interrupted_check():
  try: request('/api/w/default/trash')
  except Exception: pass
 t=threading.Thread(target=interrupted_check,daemon=True);t.start()
 deadline=time.monotonic()+30
 while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.05)
 assert (barrier/'ready').exists(),'purge did not reach crash barrier'
 stop();t.join(timeout=5);env.pop('ATLAS_TEST_NATIVE_PURGE_BARRIER');start()
 ok(200,request('/api/w/default/trash'))
 with sqlite3.connect(db) as c:assert c.execute('SELECT count(*) FROM objects WHERE id=?',(a['id'],)).fetchone()[0]==0
 assert search([b['id']]), 'wait for index after purge'
 ok(200,request('/api/w/default/trash/restore-many',{'ids':[fresh['id']]}));assert search([fresh['id']]), 'recent trash was lost during recovery'
 new=ok(201,request('/api/w/default/notes',{'title':'A','text':marker}));assert search([new['id']]);assert count()==2,('new object after expiry must be encoded again',count())
 assert not (root/'data/.default/notes'/(a['id']+'.md')).exists(), 'expired file survived recovery'
 print('PASS crash recovery, expiry and new object',root,flush=True)
finally:
 stop()
 shutil.rmtree(root)
