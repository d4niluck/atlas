#!/usr/bin/env python3
"""Source staleness and deletion in an isolated synthetic workspace."""
import argparse,http.client,json,os,pathlib,signal,subprocess,tempfile,threading,time,uuid
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--proxy',required=True);p.add_argument('--resources',required=True);p.add_argument('--workspace',choices=['local','protected','external'],required=True);args=p.parse_args()
token='atlas-test-token-0123456789abcdefghij';bootstrap='atlas-test-bootstrap-0123456789abcdef';processes=[]
def start(command,secret,env):
 p=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True,env=env);processes.append(p);p.stdin.write(secret);p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit(),('startup',port,p.stderr.read()[-1000:]);return int(port)
def request(port,method,path,body=None,headers=None):
 c=http.client.HTTPConnection('127.0.0.1',port,timeout=30)
 try:
  data=None if body is None else json.dumps(body,ensure_ascii=False).encode();c.request(method,path,data,{'Content-Type':'application/json',**(headers or {})});r=c.getresponse();raw=r.read()
  try:value=json.loads(raw) if raw else None
  except json.JSONDecodeError:value=raw[:200].decode(errors='replace')
  return r.status,dict(r.getheaders()),value
 finally:c.close()
with tempfile.TemporaryDirectory(prefix='atlas-chat-source-contract-') as tmp:
 root=pathlib.Path(tmp);models=root/'models';models.mkdir();(root/'ui').mkdir();(root/'ui'/'index.html').write_text('test')
 (models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file');(models/'gemma-4-e2b-it-q4.gguf').write_bytes(b'fake-llm')
 (models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':True,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
 env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_STREAM_FIXTURE':str(root),'DYLD_LIBRARY_PATH':args.resources,'LD_LIBRARY_PATH':args.resources})
 try:
  backend=start([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-chat.py'))],token+'\n',env)
  proxy=start([args.proxy,str(root/'ui'),str(backend),'0'],bootstrap+'\n'+token+'\n',env)
  code,headers,_=request(proxy,'GET','/bootstrap/'+bootstrap);assert code==303;auth={'Cookie':headers['Set-Cookie'].split(';',1)[0]};password='test-password-328'
  def api(method,path,body=None):
   status,_,value=request(proxy,method,path,body,auth);return status,value
  if args.workspace=='external':
   disk=root/'disk';disk.mkdir();code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'export','path':str(disk)},{'X-Atlas-Token':token});assert code==200,(code,cap)
   code,made=api('POST','/api/workspaces/create-external',{'capability':cap['capability'],'name':'External','protected':False});assert code==201,(code,made)
   code,item=api('POST','/api/workspaces/attach',{'capability':made['capability']});assert code==200,(code,item);wid=item['id']
  else:
   body={'name':'Source fixture'}
   if args.workspace=='protected':body.update({'protected':True,'password':password})
   code,item=api('POST','/api/workspaces',body);assert code==201,(code,item);wid=item['id']
  prefix='/api/w/'+wid
  code,note=api('POST',prefix+'/notes',{'title':'Источник','text':'Исходная заметка с уникальным тестовым текстом.'});assert code==201,(code,note);note_id=note['id']
  deadline=time.monotonic()+90
  while True:
   code,value=api('POST',prefix+'/search',{'mode':'semantic','query':'test','object_ids':None})
   if code==200 and note_id in [v['id'] for v in value['results']]:break
   assert code in (200,503) and time.monotonic()<deadline,(code,value);time.sleep(.1)
  def generate(n):
   reqid=str(uuid.uuid4());out=[]
   def run():
    try:out.append(api('POST',prefix+'/chat/send',{'query':'Тестовый вопрос '+str(n),'request_id':reqid}))
    except Exception as e:out.append(e)
   thread=threading.Thread(target=run,daemon=True);thread.start();deadline=time.monotonic()+30
   while not (root/('started-'+str(n))).exists():
    if not thread.is_alive():raise AssertionError(('request ended',out))
    assert time.monotonic()<deadline,out;time.sleep(.05)
   (root/('start-'+str(n))).write_text('go');(root/('finish-'+str(n))).write_text('go');thread.join(30)
   assert not thread.is_alive() and out and isinstance(out[0],tuple) and out[0][0]==200,out
   return out[0][1]
  first=generate(1);assert first['sources']==[],first
  code,_=api('POST',prefix+'/chat/clear',{});assert code==200
  (root/'single-mode').write_text('yes');second=generate(2);assert len(second['sources'])==1 and second['sources'][0]['id']==note_id,second
  def history_refs():
   code,history=api('GET',prefix+'/chat/history');assert code==200,(code,history)
   assistants=[v for v in history['messages'] if v['role']=='assistant'];assert len(assistants)==2,assistants
   found=assistants[0]['found_sources'][0];cited=assistants[1]['sources'][0]
   assert found['id']==note_id and cited['id']==note_id,(found,cited)
   return found,cited
  def reopen():
   code,value=api('POST','/api/workspaces/'+wid+'/close',{});assert code in (200,204),(code,value)
   if args.workspace=='protected':code,value=api('POST','/api/workspaces/'+wid+'/unlock',{'password':password})
   elif args.workspace=='external':
    path=next((root/'disk').glob('Atlas-*'));code,_,cap=request(backend,'POST','/api/workspaces/authorize',{'operation':'attach','path':str(path)},{'X-Atlas-Token':token});assert code==200,(code,cap)
    code,value=api('POST','/api/workspaces/attach',{'capability':cap['capability'],'id':wid})
   else:code,value=200,{}
   assert code==200,(code,value)
  found,cited=history_refs();assert not found.get('stale') and not cited.get('stale') and not found.get('missing') and not cited.get('missing'),(found,cited)
  code,value=api('PUT',prefix+'/notes/'+note_id,{'title':'Источник','text':'Совершенно другой текст в изменённой заметке.'});assert code==200,(code,value)
  found,cited=history_refs();assert found.get('stale') is True and cited.get('stale') is True,(found,cited)
  reopen();found,cited=history_refs();assert found.get('stale') is True and cited.get('stale') is True,(found,cited)
  code,value=api('POST',prefix+'/objects/delete-many',{'ids':[note_id]});assert code==200,(code,value)
  found,cited=history_refs();assert found.get('missing') is True and cited.get('missing') is True,(found,cited)
  reopen();found,cited=history_refs();assert found.get('missing') is True and cited.get('missing') is True,(found,cited)
  print('PASS changed and deleted found/cited sources',args.workspace)
 finally:
  for p in processes:
   if p.poll() is None:
    try:os.killpg(p.pid,signal.SIGTERM);p.wait(timeout=5)
    except ProcessLookupError:pass
    except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=5)
