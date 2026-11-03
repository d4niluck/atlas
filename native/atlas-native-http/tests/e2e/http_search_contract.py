#!/usr/bin/env python3
"""Isolated HTTP behavior contract; no user workspaces or real models used."""
import argparse, json, os, pathlib, signal, subprocess, tempfile, time, shutil
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',required=True);p.add_argument('--case',choices=['pdf-duplicate','history','pdf-exclusion','empty','typo','typo-common','exclude','hybrid','rag','file','image','race','hybrid-race','export','protect'],required=True);p.add_argument('--workspace',choices=['default','protected','external','protected-external'],default='default');p.add_argument('--images',action='store_true');p.add_argument('--pdf');args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-search-contract-'));models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder-file')
(models/'gemma-4-e2b-it-q4.gguf').write_bytes(b'fake-llm')
if args.images:(models/'embeddinggemma-2-mmproj-q8.gguf').write_bytes(b'fake-vision')
(models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':True,'lexical_enabled':True,'image_enabled':args.images,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}))
log=root/'calls.jsonl';token='isolated-search-contract';env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_TEST_FAKE_ENCODER_LOG':str(log),'DYLD_LIBRARY_PATH':args.resources,'LD_LIBRARY_PATH':args.resources})
if args.case=='race':env['ATLAS_RAG_TEST_BARRIER']=str(root/'barrier')
if args.case=='hybrid-race':env['ATLAS_QUERY_TEST_BARRIER']=str(root/'barrier')
proc=None;base='';wid='default';password='isolated-test-password'
def start():
 global proc,base
 proc=subprocess.Popen([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-search-model-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env)
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
def search(mode,query,ids=None):
 deadline=time.monotonic()+90
 while time.monotonic()<deadline:
  code,result=request('/api/w/'+wid+'/search',{'mode':mode,'query':query,'object_ids':ids})
  if code==200:return result['results']
  assert code==503,(code,result)
  time.sleep(.1)
 raise AssertionError(('search timeout',result))
def ids(rows):return {x['id'] for x in rows}
def set_excluded(id,value):return ok(200,request('/api/w/'+wid+'/objects/'+id+'/search-visibility',{'excluded':value},'PUT'))
try:
 start()
 if args.workspace=='protected':wid=ok(201,request('/api/workspaces',{'name':'Protected','protected':True,'password':password}))['id']
 if 'external' in args.workspace:
  disk=root/'disk';disk.mkdir();cap=ok(200,request('/api/workspaces/authorize',{'operation':'export','path':str(disk)}))['capability'];made=ok(201,request('/api/workspaces/create-external',{'capability':cap,'name':'External','protected':args.workspace=='protected-external','password':password}));wid=ok(200,request('/api/workspaces/attach',{'capability':made['capability'],**({'password':password,'test_protected':True} if args.workspace=='protected-external' else {})}))['id']
 a=ok(201,request('/api/w/'+wid+'/notes',{'title':'Документ индексирования','text':'Индексация заметок работает аккуратно. Уникальныйключ первая запись.'}));b=ok(201,request('/api/w/'+wid+'/notes',{'title':'Вторая заметка','text':'Уникальныйключ вторая запись.'}));aid=a['id'];bid=b['id']
 if args.case=='pdf-duplicate':
  pdf=pathlib.Path(args.pdf or pathlib.Path(__file__).resolve().parents[4]/'ui/tests/fixtures/two-pages.pdf').read_bytes()
  def upload_pdf():
   q=Request(base+'/api/w/'+wid+'/files?filename=manual.pdf',data=pdf,method='POST',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
   with urlopen(q,timeout=60) as r: assert r.status in (200,201); return json.load(r)
  x,y=upload_pdf(),upload_pdf();xid,yid=x['id'],y['id'];assert xid!=yid and x['title']==y['title'],(x,y)
  assert {xid,yid}.issubset(ids(search('semantic','Atlas first page')))
  original=search('lexical','Atlas first page');assert {xid,yid}.issubset(ids(original))
  set_excluded(xid,True)
  for mode in ['lexical','semantic','hybrid']:
   visible=ids(search(mode,'Atlas first page')); assert xid not in visible and yid in visible,(mode,visible,xid,yid)
   assert xid in ids(search(mode,'Atlas first page',[xid]))
  deadline=time.monotonic()+90
  while True:
   code, result=request('/api/w/'+wid+'/rag/search',{'query':'Atlas first page','object_ids':None,'agentic':False})
   if code==200:break
   assert code==503,(code,result)
   assert time.monotonic()<deadline;time.sleep(.1)
  assert xid not in ids(result['results']) and all(z.get('id')!=xid for z in result.get('sources',[])),result
  assert yid in ids(result['results']),result
  print('PASS duplicate PDF identity, RAG and explicit selection')
 elif args.case=='history':
  for mode, query in [('lexical','уникальныйключ'),('semantic','тестовый смысл'),('hybrid','проверка поиск')]:
   search(mode,query)
   data=ok(200,request('/api/w/'+wid+'/history'));assert any(row['query']==query and row['mode']==mode for row in data['items']),(mode,data)
  deadline=time.monotonic()+90
  while True:
   code, result=request('/api/w/'+wid+'/rag/search',{'query':'история уникальныйключ','object_ids':None,'agentic':False})
   if code==200:break
   assert code==503,(code,result)
   assert time.monotonic()<deadline;time.sleep(.1)
  data=ok(200,request('/api/w/'+wid+'/history'));assert any(row['query']=='история уникальныйключ' and row['mode']=='rag' for row in data['items']),data
  assert data['items'][0]['query']=='история уникальныйключ',data['items'][:2]
  stop();start()
  if args.workspace=='protected':ok(200,request('/api/workspaces/'+wid+'/unlock',{'password':password}))
  if 'external' in args.workspace:
   source=next((root/'disk').glob('Atlas-*'));cap=ok(200,request('/api/workspaces/authorize',{'operation':'attach','path':str(source)}))['capability'];wid=ok(200,request('/api/workspaces/attach',{'capability':cap,'id':wid,**({'password':password,'test_protected':True} if args.workspace=='protected-external' else {})}))['id']
  data=ok(200,request('/api/w/'+wid+'/history'));assert data['items'][0]['query']=='история уникальныйключ',data['items'][:2]
  print('PASS history saved for lexical, semantic, hybrid and RAG, newest first and after restart')
 elif args.case=='pdf-exclusion':
  pdf=pathlib.Path(args.pdf or pathlib.Path(__file__).resolve().parents[4]/'ui/tests/fixtures/two-pages.pdf').read_bytes()
  q=Request(base+'/api/w/'+wid+'/files?filename=manual.pdf',data=pdf,method='POST',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
  with urlopen(q,timeout=60) as r: assert r.status in (200,201); item=json.load(r)
  fid=item['id']; assert fid in ids(search('lexical','Atlas first page')),'PDF text not indexed';assert fid in ids(search('semantic','Atlas first page',[fid]))
  set_excluded(fid,True)
  for mode in ['lexical','semantic','hybrid']:
   assert fid not in ids(search(mode,'Atlas first page')),(mode,'excluded PDF in unscoped search')
   assert fid in ids(search(mode,'Atlas first page',[fid])),(mode,'selected PDF missing')
  print('PASS PDF exclusion, scoped override and all modes')
 elif args.case=='empty':
  search('semantic','начальная проверка')
  time.sleep(1)
  before=log.read_text() if log.exists() else ''
  empty=ok(201,request('/api/w/'+wid+'/notes',{'title':'Новая заметка','text':''}))['id']
  board=ok(200,request('/api/w/'+wid+'/board'))
  ok(200,request('/api/w/'+wid+'/board',board,'PUT'))
  time.sleep(1.5)
  search('semantic','повторная проверка')
  new=log.read_text()[len(before):] if log.exists() else ''
  assert 'Новая заметка' not in new,('empty note embedded after creation',new)
  assert empty in ids(search('lexical','Новая заметка')),'empty note title missing from lexical search'
  ok(200,request('/api/w/'+wid+'/notes/'+empty,{'title':'Важная тема','text':'Особенный текст для проверки'},'PUT'))
  search('semantic','Особенный текст для проверки')
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   text=log.read_text() if log.exists() else ''
   if 'Особенный текст для проверки' in text: break
   time.sleep(.2)
  assert 'Особенный текст для проверки' in text,'edited note not indexed'
  pasted=ok(201,request('/api/w/'+wid+'/notes',{'title':'Новая заметка','text':'Содержимое из буфера 392'}))['id']
  named=ok(201,request('/api/w/'+wid+'/notes',{'title':'Особый заголовок 675','text':''}))['id']
  search('semantic','Особый заголовок 675')
  final=log.read_text() if log.exists() else ''
  assert 'Содержимое из буфера 392' in final,'pasted note not indexed'
  assert 'Особый заголовок 675' in final,'named blank note title not indexed'
  newline=ok(201,request('/api/w/'+wid+'/notes',{'title':'Новая заметка','text':'\n'}))['id']
  search('semantic','Новое содержимое')
  final=log.read_text() if log.exists() else ''
  inputs=[json.loads(row).get('input','') for row in final.splitlines()]
  assert 'title: none | text: Новая заметка' not in inputs, 'whitespace note encoded'
  print('PASS empty creation, edit, pasted text, custom title and whitespace')
 elif args.case=='typo-common':
  special=ok(201,request('/api/w/'+wid+'/notes',{'title':'Целевой','text':'общий орхидея описание'}))['id']
  for n in range(140):ok(201,request('/api/w/'+wid+'/notes',{'title':'Отвлекающий '+str(n),'text':'общий совершенно другой материал '+str(n)}))
  rows=search('lexical','общий орхидия');assert special in ids(rows),'typo candidate is lost when common word has many matches'
  assert [x['id'] for x in rows].index(special)<5,'relevant typo correction should rank above common-word-only matches'
  print('PASS typo plus common word')
 elif args.case=='typo':
  assert aid in ids(search('lexical','индексация')),'exact should match'
  assert aid in ids(search('lexical','индексацыя')),'one typo in Cyrillic word should match'
  assert aid in ids(search('lexical','индексацтя')),'different one-character typo should match'
  assert aid in ids(search('lexical','индексация уникальныйключ')),'multiword exact should match'
  exact=ok(201,request('/api/w/'+wid+'/notes',{'title':'Точная запись','text':'marigold'}))
  typo=ok(201,request('/api/w/'+wid+'/notes',{'title':'Похожая запись','text':'marigoldx'}))
  ranked=search('lexical','marigold')
  positions=[x['id'] for x in ranked]
  assert exact['id'] in positions and typo['id'] in positions, 'exact and approximate matches must be returned'
  assert positions.index(exact['id'])<positions.index(typo['id']), 'exact result must rank above approximate'
  print('PASS typo tolerant lexical search')
 elif args.case=='exclude':
  assert ids(search('lexical','уникальныйключ'))=={aid,bid}
  assert aid in ids(search('semantic','тест',[aid,bid]))
  def encodes():
   return [json.loads(x)['input'] for x in log.read_text().splitlines() if json.loads(x).get('input','').startswith('title: none | text: ')]
  before=encodes();set_excluded(aid,True)
  assert ids(search('lexical','уникальныйключ'))=={bid},'excluded object visible in unrestricted lexical search'
  assert aid not in ids(search('semantic','тест')),'excluded object visible in unrestricted semantic search'
  assert aid not in ids(search('hybrid','уникальныйключ')),'excluded object visible in unrestricted hybrid search'
  assert aid in ids(search('lexical','уникальныйключ',[aid])),'explicit selection must override exclusion'
  assert aid in ids(search('semantic','тест',[aid])),'explicit selection must override exclusion'
  assert aid in ids(search('hybrid','уникальныйключ',[aid])),'explicit selection must override exclusion in hybrid'
  assert ids(search('lexical','уникальныйключ',[]))==set(),'empty selection must not mean all'
  assert encodes()==before,'visibility changed text embeddings'
  stop();start()
  if args.workspace=='protected':ok(200,request('/api/workspaces/'+wid+'/unlock',{'password':password}))
  if 'external' in args.workspace:
   source=next((root/'disk').glob('Atlas-*'));cap=ok(200,request('/api/workspaces/authorize',{'operation':'attach','path':str(source)}))['capability'];wid=ok(200,request('/api/workspaces/attach',{'capability':cap,'id':wid,**({'password':password,'test_protected':True} if args.workspace=='protected-external' else {})}))['id']
  assert aid not in ids(search('lexical','уникальныйключ')),'exclusion lost on restart'
  assert aid in ids(search('lexical','уникальныйключ',[aid]))
  assert aid in ids(search('semantic','тест',[aid]))
  assert encodes()==before,'restart recalculated unchanged text embeddings'
  set_excluded(aid,False)
  assert aid in ids(search('lexical','уникальныйключ')),'reenabled object missing'
  assert aid in ids(search('semantic','тест')),'reenabled object missing semantically'
  assert encodes()==before,'reenabling recalculated text embeddings'
  print('PASS exclusion, explicit selection, restart and vectors')
 elif args.case=='export':
  assert aid in ids(search('semantic','тест',[aid,bid]));set_excluded(aid,True)
  destination=root/'exports';destination.mkdir()
  if args.workspace=='protected':ok(200,request('/api/workspaces/'+wid+'/close',{}))
  cap=ok(200,request('/api/workspaces/authorize',{'operation':'export','path':str(destination)}))['capability']
  exp=ok(200,request('/api/workspaces/export',{'capability':cap,'ids':[wid],'passwords':({wid:password} if 'protected' in args.workspace else {})}))
  folder=pathlib.Path(exp.get('path') or destination/exp['folder']);cap=ok(200,request('/api/workspaces/authorize',{'operation':'import','path':str(folder)}))['capability'];out=ok(201,request('/api/workspaces/import',{'capability':cap}))['workspaces'];assert len(out)==1;wid=out[0]['id']
  if 'protected' in args.workspace:ok(200,request('/api/workspaces/'+wid+'/unlock',{'password':password}))
  assert aid not in ids(search('lexical','уникальныйключ'))
  assert aid in ids(search('lexical','уникальныйключ',[aid]))
  assert aid not in ids(search('semantic','тест'))
  assert aid in ids(search('semantic','тест',[aid]))
  print('PASS export/import visibility')
 elif args.case=='protect':
  assert args.workspace=='default';assert aid in ids(search('semantic','тест',[aid,bid]));set_excluded(aid,True)
  ok(200,request('/api/workspaces/'+wid+'/protection',{'protected':True,'password':password},'PATCH'));ok(200,request('/api/workspaces/'+wid+'/unlock',{'password':password}))
  assert aid not in ids(search('lexical','уникальныйключ'));assert aid in ids(search('lexical','уникальныйключ',[aid]));assert aid not in ids(search('semantic','тест'));assert aid in ids(search('semantic','тест',[aid]))
  ok(200,request('/api/workspaces/'+wid+'/close',{}));ok(200,request('/api/workspaces/'+wid+'/protection',{'protected':False,'password':password},'PATCH'))
  assert aid not in ids(search('lexical','уникальныйключ'));assert aid in ids(search('lexical','уникальныйключ',[aid]));assert aid not in ids(search('semantic','тест'));assert aid in ids(search('semantic','тест',[aid]))
  print('PASS protection conversion preserves visibility')
 elif args.case=='hybrid-race':
  import threading
  assert aid in ids(search('semantic','тест',[aid,bid]))
  response=[]
  def run_hybrid():response.append(request('/api/w/'+wid+'/search',{'mode':'hybrid','query':'гонка уникальныйключ', 'object_ids':None}))
  t=threading.Thread(target=run_hybrid);t.start();ready=root/'barrier.ready';deadline=time.monotonic()+30
  while not ready.exists() and time.monotonic()<deadline:time.sleep(.05)
  assert ready.exists(),'encoder barrier not reached'
  set_excluded(aid,True);(root/'barrier.go').write_text('go');t.join(timeout=30);assert not t.is_alive(),'hybrid did not complete'
  assert response and response[0][0] in (400,409),('changed scope not rejected',response)
  history=ok(200,request('/api/w/'+wid+'/history'));assert 'гонка уникальныйключ' not in json.dumps(history,ensure_ascii=False),'stale result persisted'
  print('PASS concurrent hybrid invalidation')
 elif args.case=='race':
  import threading
  assert aid in ids(search('semantic','уникальныйключ',[aid,bid]))
  response=[]
  def run_rag():response.append(request('/api/w/'+wid+'/rag/search',{'query':'Что содержит заметка?', 'object_ids':None,'agentic':False}))
  t=threading.Thread(target=run_rag);t.start();ready=root/'barrier.ready';deadline=time.monotonic()+30
  while not ready.exists() and time.monotonic()<deadline:time.sleep(.05)
  assert ready.exists(),'LLM barrier not reached'
  set_excluded(aid,True);(root/'barrier.go').write_text('go');t.join(timeout=30);assert not t.is_alive(),'RAG did not complete'
  assert response and response[0][0] in (400,409),('changed scope not rejected',response)
  history=ok(200,request('/api/w/'+wid+'/history'));assert 'Что содержит заметка?' not in json.dumps(history,ensure_ascii=False),'stale answer persisted'
  print('PASS concurrent RAG invalidation')
 elif args.case=='image':
  import base64
  assert args.images
  png=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=')
  q=Request(base+'/api/w/'+wid+'/files?filename=image.png',data=png,method='POST',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
  with urlopen(q,timeout=60) as r: assert r.status in (200,201); image=json.load(r)
  imageid=image['id'];found=search('semantic','изображение',[imageid]);assert imageid in ids(found),'image semantic index should return image'
  assert imageid in ids(search('hybrid','изображение',[imageid])),'hybrid should return image result'
  print('PASS hybrid image search')
 elif args.case=='file':
  q=Request(base+'/api/w/'+wid+'/files?filename=orchid.txt',data='пассифлора хранится в исходном файле'.encode(),method='POST',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
  with urlopen(q,timeout=60) as r: assert r.status in (200,201); f=json.load(r)
  fid=f['id'];assert fid in ids(search('lexical','пассифлора'))
  assert fid in ids(search('semantic','пассифлора',[fid]));set_excluded(fid,True)
  assert fid not in ids(search('lexical','пассифлора'))
  assert fid not in ids(search('semantic','пассифлора'))
  assert fid in ids(search('lexical','пассифлора',[fid]))
  assert fid in ids(search('semantic','пассифлора',[fid]))
  set_excluded(fid,False);assert fid in ids(search('lexical','пассифлора'))
  print('PASS file visibility')
 elif args.case=='rag':
  assert aid in ids(search('semantic','что содержит заметка?',[aid]))
  set_excluded(aid,True)
  def rag(chosen):
   deadline=time.monotonic()+90
   while time.monotonic()<deadline:
    code,res=request('/api/w/'+wid+'/rag/search',{'query':'Что содержит заметка?','object_ids':chosen,'agentic':False})
    if code==200:return res
    assert code==503,(code,res);time.sleep(.1)
   raise AssertionError(('rag timeout',res))
  no=rag(None);assert aid not in ids(no['results']),'excluded candidate exposed in RAG results'
  assert all(x.get('id')!=aid for x in no.get('sources',[])),'excluded candidate exposed in RAG sources'
  yes=rag([aid]);assert aid in ids(yes['results']),'explicitly selected excluded candidate missing from RAG results'
  assert any(x.get('id')==aid for x in yes.get('sources',[])),'explicitly selected excluded candidate missing from RAG sources'
  print('PASS RAG exclusion and explicit selection')
 elif args.case=='hybrid':
  c=ok(201,request('/api/w/'+wid+'/notes',{'title':'Третий источник','text':'Отдельный семантический источник без ключевого слова.'})); assert c['id'] in ids(search('semantic','уникальныйключ',[c['id']])), 'mock must return semantic-only candidate'
  exact=search('lexical','уникальныйключ'); semantic=search('semantic','уникальныйключ'); both=search('hybrid','уникальныйключ')
  assert ids(both)==ids(exact)|ids(semantic),'combined search omitted a source from an individual mode'
  assert ids(search('hybrid','уникальныйключ',[aid]))=={aid},'combined search violates object selection'
  print('PASS hybrid union and explicit selection')
finally:
 stop();shutil.rmtree(root)
