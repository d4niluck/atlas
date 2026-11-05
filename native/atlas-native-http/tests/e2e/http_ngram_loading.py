#!/usr/bin/env python3
import argparse,json,os,pathlib,signal,subprocess,sys,tempfile,time,uuid
from urllib.request import Request,urlopen,build_opener,ProxyHandler,install_opener
from urllib.error import HTTPError
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',required=True);a=p.parse_args()
if sys.platform!='darwin':print('SKIP Metal needs macOS');sys.exit(0)
install_opener(build_opener(ProxyHandler({})));token='atlas-ngram-loading-test'
with tempfile.TemporaryDirectory(prefix='atlas-ngram-loading-') as td:
 root=pathlib.Path(td);models=root/'models';models.mkdir();data=root/'data';gate=root/'gate';gate.mkdir();log=root/'starts.jsonl'
 for name in ('embeddinggemma-2-q8.gguf','gemma-4-e2b-it-q4.gguf','gemma-4-e4b-it-q4.gguf'):(models/name).write_bytes(b'fake-model')
 initial={'embedding_variant':'q8','embedding':'cpu','llm':'metal','semantic_enabled':True,'rag_enabled':True,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b','ngram_enabled':False}
 settings=models/'settings.json';settings.write_text(json.dumps(initial))
 fixture=pathlib.Path(__file__).with_name('mock-search-model-server.py').read_text();helper=root/'fake-server.py'
 prelude="import sys,os,time,json,pathlib\nif '--list-devices' in sys.argv:\n print('MTL0: fake Metal');sys.exit(0)\n"
 instrumentation=f"""\nwith open({str(log)!r},'a') as f:f.write(json.dumps(sys.argv[1:])+'\\n')
if '--embedding' in sys.argv: gate=pathlib.Path({str(gate/'startup')!r})
elif 'gemma-4-e4b-it-q4.gguf' in ' '.join(sys.argv): gate=pathlib.Path({str(gate/'switch')!r})
else: gate=None
if gate:
 pathlib.Path(str(gate)+'.ready').touch()
 deadline=time.monotonic()+60
 while not pathlib.Path(str(gate)+'.go').exists():
  if time.monotonic()>deadline:raise RuntimeError('gate timeout')
  time.sleep(.02)
if '--spec-type' in sys.argv and pathlib.Path({str(root/'fail-ngram')!r}).exists():sys.exit(2)
"""
 fixture=fixture.replace('import sys,socketserver,http.server,json,os,time,pathlib',prelude+'\nimport sys,socketserver,http.server,json,os,time,pathlib').replace('with S(path,H) as s:s.serve_forever()',instrumentation+'\nwith S(path,H) as s:s.serve_forever()')
 helper.write_text(fixture);helper.chmod(0o700)
 env=os.environ.copy();env.update(DYLD_LIBRARY_PATH=a.resources,LD_LIBRARY_PATH=a.resources,ATLAS_TEST_FAKE_ENCODER_LOG=str(root/'calls.jsonl'),ATLAS_NGRAM_STARTUP_BARRIER=str(gate))
 proc=subprocess.Popen([a.backend,'--root',str(data),'--port','0','--rag-model-dir',str(models),'--llama-server',str(helper)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True,env=env)
 def wait_for(predicate,label):
  deadline=time.monotonic()+35
  while time.monotonic()<deadline:
   if predicate():return
   if proc.poll() is not None:raise AssertionError('backend exited: '+proc.stderr.read(500))
   time.sleep(.05)
  raise AssertionError('timeout: '+label)
 def request(path,method='GET',payload=None):
  body=None if payload is None else json.dumps(payload).encode();q=Request(base+path,data=body,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
  try:
   with urlopen(q,timeout=15) as r:return r.status,json.load(r)
  except HTTPError as e:return e.code,json.load(e)
 try:
  proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),port;base='http://127.0.0.1:'+port
  wait_for(lambda:(gate/'.ready').exists(),'startup gate')
  unrelated=dict(initial,idle_minutes=30);code,_=request('/api/settings/app','PUT',unrelated);assert code==200,code;assert json.loads(settings.read_text())['idle_minutes']==30
  legacy=dict(unrelated,idle_minutes=31);legacy.pop('ngram_enabled');code,_=request('/api/settings/app','PUT',legacy);assert code==200,code;assert json.loads(settings.read_text())['ngram_enabled'] is False
  changed=dict(unrelated,idle_minutes=31,ngram_enabled=True);before=settings.read_bytes()
  for edit in (changed,dict(unrelated,idle_minutes=31,llm='cpu'),dict(unrelated,idle_minutes=31,model='e4b'),dict(unrelated,idle_minutes=31,rag_enabled=False)):
   code,_=request('/api/settings/app','PUT',edit);assert code==409,(code,edit);assert settings.read_bytes()==before
  (gate/'.go').touch();wait_for(lambda:request('/api/settings/device')[1].get('startup_complete') is True,'startup completion')
  assert request('/api/settings/app','PUT',changed)[0]==200
  assert request('/api/w/default/rag/model','POST',{'model':'e4b'})[0]==202
  wait_for(lambda:(gate/'switch.ready').exists(),'model switch gate')
  before=settings.read_bytes();disabled=dict(changed,ngram_enabled=False)
  unrelated=dict(changed,idle_minutes=32);legacy=dict(unrelated);legacy.pop('ngram_enabled')
  for edit in (unrelated,legacy,disabled,dict(changed,llm='cpu'),dict(changed,model='e4b'),dict(changed,rag_enabled=False)):
   code,_=request('/api/settings/app','PUT',edit);assert code==409,(code,edit);assert settings.read_bytes()==before
  (gate/'switch.go').touch();wait_for(lambda:request('/api/w/default/rag/status')[1].get('model')=='e4b','model switch completion')
  args_log=[json.loads(line) for line in log.read_text().splitlines()];assert any('--spec-type' in line and any('gemma-4-e4b' in x for x in line) for line in args_log),args_log
  disabled['model']='e4b';assert request('/api/settings/app','PUT',disabled)[0]==200;assert json.loads(settings.read_text())['ngram_enabled'] is False
  print('startup and model-switch settings races passed')
  enabled=dict(disabled,ngram_enabled=True);assert request('/api/settings/app','PUT',enabled)[0]==200
  (root/'fail-ngram').touch();(gate/'startup.go').touch()
  code,_=request('/api/w/default/notes','POST',{'title':'Тест отказа','text':'Абрикос — тестовый фрукт для проверки поиска.'});assert code==201,code
  def index_ready():
   code,_=request('/api/w/default/search','POST',{'mode':'semantic','query':'абрикос','object_ids':None})
   assert code in (200,503),code
   return code==200
  wait_for(index_ready,'semantic index')
  query={'query':'Что такое абрикос?','object_ids':None,'agentic':False}
  code,result=request('/api/w/default/rag/search','POST',query);assert code==503,(code,result);assert 'ускорен' in result['error'].lower(),result
  reqid=str(uuid.uuid4());code,result=request('/api/w/default/rag/search','POST',dict(query,request_id=reqid));assert code==503,(code,result);assert 'ускорен' in result['error'].lower(),result
  code,events=request('/api/w/default/rag/events/'+reqid);assert code==200,(code,events);assert any('ускорен' in json.dumps(event,ensure_ascii=False).lower() for event in events['events']),events
  assert request('/api/w/default/rag/model','POST',{'model':'e4b'})[0]==202
  wait_for(lambda:'ускорен' in request('/api/w/default/rag/status')[1].get('message','').lower(),'failed accelerated model')
  assert request('/api/settings/app','PUT',disabled)[0]==200
  code,status=request('/api/w/default/rag/status');assert code==200 and 'не удалось запустить' not in status.get('message','').lower(),status
  (root/'fail-ngram').unlink();code,result=request('/api/w/default/rag/search','POST',query);assert code==200,(code,result)
  print('accelerated startup errors, stream event and recovery passed')
 finally:
  if proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=15)
   except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=10)
