#!/usr/bin/env python3
"""Isolated chat HTTP contract with a real proxy and synthetic model.

Run: python3 http_chat_contract.py --backend PATH --proxy PATH --resources DIR
"""
import argparse, http.client, json, os, pathlib, signal, subprocess, tempfile

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
  backend=start([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-stream.py'))],token+'\n',env)
  proxy=start([args.proxy,str(root/'ui'),str(backend),'0'],bootstrap+'\n'+token+'\n',env)
  code,headers,_=request(proxy,'GET','/bootstrap/'+bootstrap);assert code==303,(code,headers)
  auth={'Cookie':headers['Set-Cookie'].split(';',1)[0]};prefix='/api/w/default'
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth)
  assert code==200 and history['messages']==[] and isinstance(history['summary'],str),('new chat must start empty',code,history)
  code,_,_=request(proxy,'GET',prefix+'/chat/history');assert code in (401,403,404),('history exposed without session',code)
  code,_,value=request(proxy,'POST',prefix+'/chat/clear',{},auth);assert code==200 and value.get('ok') is True,(code,value)
  code,_,history=request(proxy,'GET',prefix+'/chat/history',headers=auth);assert code==200 and history['messages']==[] and history['summary']=='',(code,history)
  print('PASS empty chat, session authorization and clearing')
 finally:
  for process in processes:
   if process.poll() is None:
    try:os.killpg(process.pid,signal.SIGTERM);process.wait(timeout=5)
    except ProcessLookupError:pass
    except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=5)
