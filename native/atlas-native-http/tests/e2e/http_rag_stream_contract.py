#!/usr/bin/env python3
"""Black-box RAG streaming contract, using synthetic model and a real Atlas proxy.

Usage: python3 check.py --backend PATH --proxy PATH --resources DIR
All workspaces and model data are temporary. Requires local loopback access.
"""
import argparse, http.client, json, os, pathlib, signal, subprocess, sys, tempfile, threading, time, uuid

ap=argparse.ArgumentParser();ap.add_argument('--backend',required=True);ap.add_argument('--proxy',required=True);ap.add_argument('--resources',required=True);ap.add_argument('--workspace',choices=['default','protected','external','protected-external'],default='default');ap.add_argument('--stalled-ipc',action='store_true');args=ap.parse_args()
token='atlas-test-token-0123456789abcdefghij'; bootstrap='atlas-test-bootstrap-0123456789abcdef'; processes=[]

def wait_for(predicate, timeout=30, what='condition'):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        value=predicate()
        if value: return value
        time.sleep(.05)
    raise AssertionError('timeout waiting for '+what)

def start(command, stdin, env):
    p=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True,env=env)
    processes.append(p);p.stdin.write(stdin);p.stdin.flush()
    port=p.stdout.readline().strip()
    if not port.isdigit(): raise AssertionError('process start failed: '+port+' '+p.stderr.read()[-2000:])
    return int(port)

def request(port,method,path,body=None,headers=None,timeout=20):
    data=None if body is None else json.dumps(body,ensure_ascii=False).encode()
    h={'Content-Type':'application/json',**(headers or {})}
    c=http.client.HTTPConnection('127.0.0.1',port,timeout=timeout)
    try:
        c.request(method,path,body=data,headers=h);r=c.getresponse();raw=r.read();return r.status,dict(r.getheaders()),json.loads(raw) if raw else None
    finally:c.close()

with tempfile.TemporaryDirectory(prefix='atlas-rag-stream-test-') as tmp:
 root=pathlib.Path(tmp); models=root/'models';models.mkdir();(root/'ui').mkdir();(root/'ui'/'index.html').write_text('test')
 (models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file');(models/'gemma-4-e2b-it-q4.gguf').write_bytes(b'fake-llm')
 (models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':True,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
 env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_STREAM_FIXTURE':str(root),'DYLD_LIBRARY_PATH':args.resources,'LD_LIBRARY_PATH':args.resources})
 paused_worker=None
 try:
  backend=start([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-stream.py'))],token+'\n',env)
  proxy=start([args.proxy,str(root/'ui'),str(backend),'0'],bootstrap+'\n'+token+'\n',env)
  c=http.client.HTTPConnection('127.0.0.1',proxy,timeout=10);c.request('GET','/bootstrap/'+bootstrap);r=c.getresponse();r.read();assert r.status==303,(r.status,r.getheaders());cookie=r.getheader('Set-Cookie').split(';',1)[0];c.close()
  h={'Cookie':cookie}
  def api(method,path,body=None):
   status,_,value=request(proxy,method,path,body,h);return status,value
  wid='default';password='synthetic-test-password'
  if args.workspace=='protected':
   code,result=api('POST','/api/workspaces',{'name':'Protected','protected':True,'password':password});assert code==201,(code,result);wid=result['id']
  if 'external' in args.workspace:
   disk=root/'disk';disk.mkdir()
   code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'export','path':str(disk)},{'X-Atlas-Token':token});assert code==200,(code,cap)
   code,made=api('POST','/api/workspaces/create-external',{'capability':cap['capability'],'name':'External','protected':args.workspace=='protected-external','password':password});assert code==201,(code,made)
   code,result=api('POST','/api/workspaces/attach',{'capability':made['capability'],**({'password':password,'test_protected':True} if args.workspace=='protected-external' else {})});assert code==200,(code,result);wid=result['id']
  prefix='/api/w/'+wid
  code,note=api('POST',prefix+'/notes',{'title':'Проверка стриминга','text':'Стриминг выдаёт этот фрагмент до начала ответа модели. Уникальное слово абрикос.'});assert code==201,(code,note)
  def ready():
   code,value=api('POST',prefix+'/search',{'mode':'semantic','query':'абрикос','object_ids':None})
   if code==200:return value
   assert code==503,(code,value)
  wait_for(ready,90,'semantic index')
  search_prefix=prefix+'/rag/'
  def events(reqid,after=0):
   code,value=api('GET',search_prefix+'events/'+reqid+'?after='+str(after))
   if code==404:return []
   assert code==200,(code,value);return value['events']
  def run_search(reqid,query):
   output=[]
   def run():
    try:output.append(api('POST',search_prefix+'search',{'query':query,'object_ids':None,'agentic':False,'request_id':reqid}))
    except Exception as e:output.append(e)
   t=threading.Thread(target=run,daemon=True);t.start();return t,output
  if args.stalled_ipc:
   assert 'external' in args.workspace,'stalled-ipc requires external workspace'
   backend_pid=processes[0].pid
   def worker():
    lines=subprocess.check_output(['ps','-axo','pid=,ppid=,command='],text=True).splitlines()
    for line in lines:
     parts=line.strip().split(None,2)
     if len(parts)==3 and int(parts[1])==backend_pid and ('--workspace-worker' in parts[2] or '--protected-workspace-worker' in parts[2]): return int(parts[0])
   paused_worker=wait_for(worker,10,'external workspace worker');os.kill(paused_worker,signal.SIGSTOP)
   blocked_id=str(uuid.uuid4());blocked_query='Остановка во время IPC абрикос';blocked,blocked_out=run_search(blocked_id,blocked_query)
   time.sleep(.8);assert blocked.is_alive(),'search unexpectedly finished before IPC resumed'
   start_cancel=time.monotonic();code,value=api('POST',search_prefix+'cancel/'+blocked_id,{})
   assert code in (200,202,204),(code,value);assert time.monotonic()-start_cancel<3,'cancel API blocked on stalled IPC'
   os.kill(paused_worker,signal.SIGCONT);paused_worker=None;blocked.join(20)
   assert not blocked.is_alive(),('cancelled search stayed blocked after worker resumed',blocked_out)
   time.sleep(.4);assert not (root/'started-1').exists(),'LLM started after IPC completed despite cancellation'
   code,history=api('GET',prefix+'/history');assert code==200 and all(x['query']!=blocked_query for x in history['items']),history
   print('PASS cancelling during stalled external IPC prevents subsequent LLM and history')
   sys.exit(0)
  reqid=str(uuid.uuid4());query='Какой смысл у абрикоса?';thread,out=run_search(reqid,query)
  initial=wait_for(lambda:next((x for x in events(reqid) if x['type']=='results'),None),60,'initial retrieval event')
  assert initial['results'] and any(row.get('id')==note['id'] for row in initial['results']),initial
  wait_for(lambda:(root/'started-1').exists(),60,'LLM start');assert thread.is_alive(),'search finished before model was released'
  code,opened=api('GET',prefix+'/notes/'+note['id']);assert code==200 and 'абрикос' in opened['text'],('early result source unavailable',code,opened)
  assert thread.is_alive(),'model unexpectedly finished while opening early source'
  assert not any(x['type']=='token' for x in events(reqid)),'token appeared before model produced one'
  (root/'start-1').write_text('go')
  first=wait_for(lambda:next((x for x in events(reqid) if x['type']=='token'),None),20,'first streamed token')
  assert first['text']=='Первая ',first;assert thread.is_alive(),'search finished before second token was allowed'
  (root/'finish-1').write_text('go');thread.join(30);assert not thread.is_alive(),out
  assert out and isinstance(out[0],tuple) and out[0][0]==200,out
  ev=events(reqid);tokens=''.join(x['text'] for x in ev if x['type']=='token');assert 'Первая часть' in tokens,ev
  assert any(x['type']=='done' for x in ev),ev
  code,history=api('GET',prefix+'/history');assert code==200 and any(x['query']==query for x in history['items']),history
  print('PASS actual proxy: early candidates, incremental answer and completed history')
  if args.workspace=='protected':
   code,_=api('POST','/api/workspaces/'+wid+'/close',{});assert code in (200,204),code
   code,_=api('GET',search_prefix+'events/'+reqid+'?after=0');assert code in (403,404),('events exposed after lock',code)
   code,_=api('POST','/api/workspaces/'+wid+'/unlock',{'password':password});assert code==200,code
   code,_=api('GET',search_prefix+'events/'+reqid+'?after=0');assert code==404,('previous session events exposed after unlock',code)
   wait_for(ready,90,'protected semantic index after unlock')
   print('PASS protected workspace: previous stream hidden after close and unlock')
  reqid2=str(uuid.uuid4());query2='Отменить абрикос';thread2,out2=run_search(reqid2,query2)
  wait_for(lambda:next((x for x in events(reqid2) if x['type']=='results'),None),60,'second retrieval')
  wait_for(lambda:(root/'started-2').exists(),60,'second model request');(root/'start-2').write_text('go')
  wait_for(lambda:next((x for x in events(reqid2) if x['type']=='token'),None),20,'second token')
  reqid3=str(uuid.uuid4());query3='Очередь абрикос';thread3,out3=run_search(reqid3,query3)
  wait_for(lambda:next((x for x in events(reqid3) if x['type']=='results'),None),60,'queued retrieval')
  code,value=api('POST',search_prefix+'cancel/'+reqid3,{})
  assert code in (200,202,204),(code,value);thread3.join(8)
  assert not thread3.is_alive(), 'cancelled queued request still waits for LLM mutex'
  assert thread2.is_alive(), 'cancelling queued request stopped another request'
  before=time.monotonic();code,value=api('POST',search_prefix+'cancel/'+reqid2,{})
  assert code in (200,202,204),(code,value);thread2.join(8)
  if thread2.is_alive():(root/'finish-2').write_text('go');raise AssertionError('cancel did not stop search promptly')
  assert time.monotonic()-before<8
  (root/'finish-2').write_text('go')
  code,history=api('GET',prefix+'/history');assert code==200 and all(x['query'] not in (query2,query3) for x in history['items']),history
  print('PASS cancelling active and queued requests releases them without saving incomplete answers')
  reqid4=str(uuid.uuid4());query4='Отмена до старта абрикос';code,value=api('POST',search_prefix+'cancel/'+reqid4,{})
  assert code in (200,202,204),(code,value)
  thread4,out4=run_search(reqid4,query4);thread4.join(8)
  assert not thread4.is_alive(),'request cancelled before registration nevertheless started'
  assert not (root/'started-3').exists(),'model started after request was already cancelled'
  code,history=api('GET',prefix+'/history');assert code==200 and all(x['query']!=query4 for x in history['items']),history
  print('PASS cancellation before request registration')
  if args.workspace in ('protected','external','protected-external'):
   reqid5=str(uuid.uuid4());query5='Закрыть пространство абрикос';thread5,out5=run_search(reqid5,query5)
   wait_for(lambda:next((x for x in events(reqid5) if x['type']=='results'),None),60,'retrieval before closing workspace')
   wait_for(lambda:(root/'started-3').exists(),60,'model before workspace close')
   (root/'start-3').write_text('go');wait_for(lambda:next((x for x in events(reqid5) if x['type']=='token'),None),20,'token before workspace close')
   code,value=api('POST','/api/workspaces/'+wid+'/close',{});assert code in (200,204),(code,value)
   thread5.join(10);assert not thread5.is_alive(),'closing workspace did not cancel generation'
   (root/'finish-3').write_text('go')
   code,_=api('GET',search_prefix+'events/'+reqid5+'?after=0');assert code in (403,404),('events exposed after workspace close',code)
   if args.workspace=='protected':
    code,_=api('POST','/api/workspaces/'+wid+'/unlock',{'password':password});assert code==200,code
   else:
    source=next((root/'disk').glob('Atlas-*'))
    code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'attach','path':str(source)},{'X-Atlas-Token':token});assert code==200,(code,cap)
    code,result=api('POST','/api/workspaces/attach',{'capability':cap['capability'],'id':wid,**({'password':password,'test_protected':True} if args.workspace=='protected-external' else {})});assert code==200,(code,result)
   code,_=api('GET',search_prefix+'events/'+reqid5+'?after=0');assert code==404,('old events exposed after reopening',code)
   code,history=api('GET',prefix+'/history');assert code==200 and all(x['query']!=query5 for x in history['items']),history
   print('PASS closing workspace cancels generation and prevents old events after reopening')
 finally:
  if paused_worker is not None:
   try:os.kill(paused_worker,signal.SIGCONT)
   except ProcessLookupError:pass
  for p in processes:
   if p.poll() is None:
    try:os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=5)
    except ProcessLookupError:pass
    except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=5)
