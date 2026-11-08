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
 root=pathlib.Path(tmp); models=root/'models';models.mkdir();(root/'ui').mkdir();(root/'ui'/'index.html').write_text('test')
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
  assert code==200 and history['messages']==[] and isinstance(history['summary'],str),('new chat must start empty',code,history)
  code,_,_=request(proxy,'GET',prefix+'/chat/history');assert code in (401,403,404),('history exposed without session',code)
  code,_,value=request(proxy,'POST',prefix+'/chat/clear',{},auth);assert code==200 and value.get('ok') is True,(code,value)
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200 and history['messages']==[] and history['summary']=='',(code,history)
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
  assert answer=='Первая часть.' and any(e.get('type')=='done' for e in seen),seen
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200,(code,history)
  messages=history['messages'];assert len(messages)==2,messages
  assert messages[0]['role']=='user' and messages[0]['content']==query,messages
  assert messages[1]['role']=='assistant' and messages[1]['content']==answer,messages
  reqid2=str(uuid.uuid4());cancelled_query='cancel-synthetic-secret-538cf';cancelled_output=[]
  def send_cancel():
   try:cancelled_output.append(request(proxy,'POST',prefix+'/chat/send',{'query':cancelled_query,'request_id':reqid2},auth))
   except Exception as e:cancelled_output.append(e)
  second=threading.Thread(target=send_cancel,daemon=True);second.start()
  wait(lambda:(root/'started-2').exists(),'second generation');(root/'start-2').write_text('go')
  wait(lambda:(root/'first-2').exists(),'second token')
  code,_,value=request(proxy,'POST',prefix+'/rag/cancel/'+reqid2,{},auth);assert code in (200,202,204),(code,value)
  second.join(10);assert not second.is_alive(),'cancelled chat remained active'
  (root/'finish-2').write_text('go')
  code,_,after_cancel=request(proxy,'GET',prefix+'/chat/history',headers=auth)
  assert code==200 and after_cancel['messages']==messages,('cancel saved a partial turn',code,after_cancel)
  password='synthetic-password-2df4'
  code,_,value=request(proxy,'PATCH','/api/workspaces/'+wid+'/protection',{'protected':True,'password':password},auth);assert code==200,(code,value)
  code,_,value=request(proxy,'POST','/api/workspaces/'+wid+'/close',{},auth);assert code in (200,204),(code,value)
  # No user text may remain in the unencrypted registry, temporary files or logs.
  for candidate in root.rglob('*'):
   if candidate.is_file():assert query.encode() not in candidate.read_bytes(),('chat leaked to disk',str(candidate.relative_to(root)))
  code,_,_=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code in (403,404),code
  encrypted_dest=root/'protected-exports';encrypted_dest.mkdir()
  code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'export','path':str(encrypted_dest)},{'X-Atlas-Token':token});assert code==200,(code,cap)
  code,_,exported=request(proxy,'POST','/api/workspaces/export',{'capability':cap['capability'],'ids':[wid],'passwords':{wid:password}},auth);assert code==200,(code,exported)
  encrypted_copy=pathlib.Path(exported.get('path') or encrypted_dest/exported['folder'])
  code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'import','path':str(encrypted_copy)},{'X-Atlas-Token':token});assert code==200,(code,cap)
  code,_,imported=request(proxy,'POST','/api/workspaces/import',{'capability':cap['capability']},auth);assert code==201,(code,imported)
  copied_protected=imported['workspaces'][0]['id'];copy_prefix='/api/w/'+copied_protected
  code,_,_=request(proxy,'GET',copy_prefix+'/chat/history',headers=auth);assert code in (403,404),code
  code,_,value=request(proxy,'POST','/api/workspaces/'+copied_protected+'/unlock',{'password':password},auth);assert code==200,(code,value)
  code,_,copy_history=request(proxy,'GET',copy_prefix+'/chat/history',headers=auth);assert code==200 and copy_history['messages']==messages,('protected copy lost chat',code,copy_history)
  code,_,value=request(proxy,'POST','/api/workspaces/'+copied_protected+'/close',{},auth);assert code in (200,204),(code,value)
  for candidate in root.rglob('*'):
   if candidate.is_file():assert query.encode() not in candidate.read_bytes(),('protected export leaked chat',str(candidate.relative_to(root)))
  code,_,value=request(proxy,'POST','/api/workspaces/'+wid+'/unlock',{'password':password},auth);assert code==200,(code,value)
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200 and history['messages']==messages,(code,history)
  code,_,value=request(proxy,'POST','/api/workspaces/'+wid+'/close',{},auth);assert code in (200,204),(code,value)
  code,_,value=request(proxy,'PATCH','/api/workspaces/'+wid+'/protection',{'protected':False,'password':password},auth);assert code==200,(code,value)
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200 and history['messages']==messages,(code,history)
  destination=root/'exports';destination.mkdir()
  code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'export','path':str(destination)},{'X-Atlas-Token':token});assert code==200,(code,cap)
  code,_,exported=request(proxy,'POST','/api/workspaces/export',{'capability':cap['capability'],'ids':[wid]},auth);assert code==200,(code,exported)
  folder=pathlib.Path(exported.get('path') or destination/exported['folder'])
  code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'import','path':str(folder)},{'X-Atlas-Token':token});assert code==200,(code,cap)
  code,_,imported=request(proxy,'POST','/api/workspaces/import',{'capability':cap['capability']},auth);assert code==201,(code,imported)
  copied=imported['workspaces'][0]['id'];code,_,copy_history=request(proxy,'GET','/api/w/'+copied+'/chat/history',headers=auth)
  assert code==200 and copy_history['messages']==messages,('chat history lost during copy',code,copy_history)
  code,_,value=request(proxy,'POST',prefix+'/chat/clear',{},auth);assert code==200 and value.get('ok') is True,(code,value)
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200 and history['messages']==messages and history['summary']=='',(code,history)
  code,_,copy_history=request(proxy,'GET','/api/w/'+copied+'/chat/history',headers=auth);assert code==200 and copy_history['messages']==messages,('clear affected copied workspace',code,copy_history)
  print('PASS chat streaming, cancellation, encrypted conversion, copy and context clearing')
 finally:
  for process in processes:
   if process.poll() is None:
    try:os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=5)
    except ProcessLookupError:pass
    except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
   diagnostics=process.stderr.read()
   assert 'chat-synthetic-secret-84c719' not in diagnostics and 'cancel-synthetic-secret-538cf' not in diagnostics, 'chat text leaked to process logs'
