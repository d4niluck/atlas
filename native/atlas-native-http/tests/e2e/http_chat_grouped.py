#!/usr/bin/env python3
"""Isolated chat HTTP contract with a real proxy and synthetic model.

Run: python3 http_chat_contract.py --backend PATH --proxy PATH --resources DIR
"""
import argparse, http.client, json, os, pathlib, signal, subprocess, tempfile, threading, time, uuid

p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--proxy',required=True);p.add_argument('--resources',required=True);args=p.parse_args()
token='atlas-test-token-0123456789abcdefghij'; bootstrap='atlas-test-bootstrap-0123456789abcdef'; processes=[]

def start(command,secret,env):
 p=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True,env=env);processes.append(p)
 p.stdin.write(secret);p.stdin.flush();port=p.stdout.readline().strip()
 assert port.isdigit(),('process start failed',port,p.stderr.read()[-1000:]);return int(port)

def request(port,method,path,body=None,headers=None):
 c=http.client.HTTPConnection('127.0.0.1',port,timeout=30)
 try:
  data=None if body is None else json.dumps(body,ensure_ascii=False).encode()
  c.request(method,path,data,{'Content-Type':'application/json',**(headers or {})});r=c.getresponse();raw=r.read()
  try:value=json.loads(raw) if raw else None
  except json.JSONDecodeError:value=raw[:200].decode(errors='replace')
  return r.status,dict(r.getheaders()),value
 finally:c.close()

with tempfile.TemporaryDirectory(prefix='atlas-chat-contract-') as tmp:
 root=pathlib.Path(tmp); models=root/'models';models.mkdir();(root/'ui').mkdir();(root/'ui'/'index.html').write_text('test');(root/'group-mode').write_text('yes')
 (models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file');(models/'gemma-4-e2b-it-q4.gguf').write_bytes(b'fake-llm')
 (models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':True,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
 env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_STREAM_FIXTURE':str(root),'DYLD_LIBRARY_PATH':args.resources,'LD_LIBRARY_PATH':args.resources})
 try:
  backend=start([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-chat.py'))],token+'\n',env)
  proxy=start([args.proxy,str(root/'ui'),str(backend),'0'],bootstrap+'\n'+token+'\n',env)
  code,headers,_=request(proxy,'GET','/bootstrap/'+bootstrap);assert code==303,(code,headers)
  auth={'Cookie':headers['Set-Cookie'].split(';',1)[0]}
  code,_,made=request(proxy,'POST','/api/workspaces',{'name':'chat fixture'},auth);assert code==201,(code,made)
  wid=made['id'];prefix='/api/w/'+wid
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth)
  assert code==200 and history['messages']==[] and history['next_before'] is None,('new chat must start empty',code,history)
  code,_,_=request(proxy,'GET',prefix+'/chat/history');assert code in (401,403,404),('history exposed without session',code)
  code,_,value=request(proxy,'POST',prefix+'/chat/clear',{},auth);assert code==200 and value.get('ok') is True,(code,value)
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200 and history['messages']==[] and history['next_before'] is None,(code,history)
  code,_,note=request(proxy,'POST',prefix+'/notes',{'title':'fixture','text':'Тестовая заметка для проверки видимости.'},auth);assert code==201,(code,note);note_id=note['id']
  code,_,note2=request(proxy,'POST',prefix+'/notes',{'title':'fixture2','text':'Ещё одна тестовая заметка для проверки поиска.'},auth);assert code==201,(code,note2)
  deadline=time.monotonic()+90
  while True:
   code,_,search=request(proxy,'POST',prefix+'/search',{'mode':'semantic','query':'test','object_ids':None},auth)
   if code==200 and {note_id,note2['id']}.issubset({x['id'] for x in search['results']}):break
   assert code in (200,503) and time.monotonic()<deadline,(code,search)
   time.sleep(.1)
  reqid=str(uuid.uuid4());query='chat-synthetic-secret-84c719';output=[]
  def send():
   try:output.append(request(proxy,'POST',prefix+'/chat/send',{'query':query,'request_id':reqid},auth))
   except Exception as e:output.append(e)
  thread=threading.Thread(target=send,daemon=True);thread.start()
  def wait(predicate,label,seconds=90):
   deadline=time.monotonic()+seconds
   while time.monotonic()<deadline:
    value=predicate()
    if value:return value
    if label=='LLM start' and not thread.is_alive():raise AssertionError(('chat ended before generation',output))
    if label=='generation after context clearing' and not third.is_alive():raise AssertionError(('chat after clear ended before generation',third_output))
    time.sleep(.05)
   raise AssertionError('timeout waiting for '+label)
  def events():
   code,_,value=request(proxy,'GET',prefix+'/rag/events/'+reqid+'?after=0',headers=auth)
   if code==404:return []
   assert code==200,(code,value);return value['events']
  wait(lambda:(root/'started-1').exists(),'LLM start');(root/'start-1').write_text('go')
  wait(lambda:any(event.get('type')=='token' for event in events()),'first token')
  assert thread.is_alive(),'answer did not stream'
  (root/'finish-1').write_text('go');thread.join(30)
  assert not thread.is_alive() and output and isinstance(output[0],tuple) and output[0][0]==200,output
  seen=events();answer=''.join(e['text'] for e in seen if e.get('type')=='token')
  assert any(e.get('type')=='done' for e in seen),seen
  answer=output[0][2]['answer']
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200,(code,history)
  messages=history['messages'];assert len(messages)==2,messages
  assert messages[0]['role']=='user' and messages[0]['content']==query,messages
  assert messages[1]['role']=='assistant' and messages[1]['content']==answer,messages
  assert answer=='Первая часть [1][2].',('grouped references not normalized',answer)
  sources=output[0][2]['sources'];assert len(sources)==2,('grouped references lost sources',sources)
  assert sources[0]['id']!=sources[1]['id'],sources
  print('PASS grouped citations linked to two distinct sources')
 finally:
  for process in processes:
   if process.poll() is None:
    try:os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=5)
    except ProcessLookupError:pass
    except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
