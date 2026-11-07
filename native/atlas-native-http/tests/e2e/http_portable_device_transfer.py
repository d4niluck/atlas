#!/usr/bin/env python3
"""Test model identity across portable workspace exports, with synthetic encoder only."""
import argparse, base64, concurrent.futures, hashlib, json, os, pathlib, shutil, signal, subprocess, tempfile, time
from urllib.request import Request, urlopen, install_opener, build_opener, ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--source-backend');p.add_argument('--resources',default='');p.add_argument('--protected',action='store_true');p.add_argument('--external',action='store_true');p.add_argument('--attach-export',action='store_true');p.add_argument('--corrupt-worker',action='store_true');p.add_argument('--require-direct',action='store_true');p.add_argument('--stale-direct',action='store_true');p.add_argument('--symlink-direct',action='store_true');p.add_argument('--io-error-direct',action='store_true');p.add_argument('--search-io-error-direct',action='store_true');p.add_argument('--worker-reopen',action='store_true');p.add_argument('--worker-reopen-stale',action='store_true');p.add_argument('--cancel-direct',action='store_true');p.add_argument('--image-replace-direct',action='store_true');p.add_argument('--from-device',choices=['cpu','metal'],default='cpu');p.add_argument('--compatible-models',nargs=2,metavar=('Q8','BF16'),help='known tested model files, read-only');p.add_argument('--start-bf16',action='store_true');args=p.parse_args();assert not args.start_bf16 or args.compatible_models, '--start-bf16 requires --compatible-models'
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-index-independent-'));models=root/'models';models.mkdir();log=root/'calls.jsonl';actions=root/'index-actions.log'
q8=models/'embeddinggemma-2-q8.gguf';bf16=models/'embeddinggemma-2.gguf'
if args.compatible_models:
 q8.symlink_to(pathlib.Path(args.compatible_models[0]).resolve(strict=True));bf16.symlink_to(pathlib.Path(args.compatible_models[1]).resolve(strict=True))
else:
 q8.write_bytes(b'Q8-model-v1');bf16.write_bytes(b'BF16-model-v1')
(models/'embeddinggemma-2-mmproj-q8.gguf').write_bytes(b'fake-mmproj')
mock=pathlib.Path(__file__).with_name('mock-portable-embedding-server.py')
if args.image_replace_direct:
 assert args.attach_export and args.require_direct and not args.protected
 source=mock.read_text().replace("text=value['input'];","text=value['input'];text=json.dumps(text,sort_keys=True) if isinstance(text,list) else text;")
 mock=root/'mock-with-images.py';mock.write_text(source);mock.chmod(0o700)
settings={'embedding_variant':'bf16' if args.start_bf16 else 'q8','embedding':args.from_device,'llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':args.image_replace_direct,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'};(models/'settings.json').write_text(json.dumps(settings));marker='UNIQUE_PORTABLE_VECTOR_ALPHA_7381';password='test-password-01';token='independent-test-token';env=os.environ.copy();env.update(NO_PROXY='*',ATLAS_TEST_FAKE_ENCODER_LOG=str(log),ATLAS_TEST_INDEX_ACTION_LOG=str(actions),DYLD_LIBRARY_PATH=args.resources,LD_LIBRARY_PATH=args.resources);proc=None;data_root=root/'data'

def req(path,body=None,method=None):
 data=None if body is None else json.dumps(body,ensure_ascii=False).encode();q=Request(base+path,data=data,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
 try:
  with urlopen(q,timeout=30) as r:return r.status,json.load(r)
 except HTTPError as e:
  try:return e.code,json.loads(e.read())
  except Exception:return e.code,{}
def ok(status,r):assert r[0]==status,r;return r[1]
def count():
 if not log.exists():return 0
 return sum(marker in x.get('input','') and x['input'].startswith('title: none | text: ') for x in map(json.loads,log.read_text().splitlines()))
def search(wid,obj):
 end=time.monotonic()+(20 if args.corrupt_worker else 120)
 while time.monotonic()<end:
  code,data=req('/api/w/'+wid+'/search',{'mode':'semantic','query':'тестовый вопрос','object_ids':[obj]})
  if code==200:return data['results']
  assert code==503,(code,data);time.sleep(.1)
 raise AssertionError(('timeout',data))
def cap(op,path):return ok(200,req('/api/workspaces/authorize',{'operation':op,'path':str(path)}))['capability']
try:
 proc=subprocess.Popen([args.source_backend or args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('backend failed',port);base='http://127.0.0.1:'+port
 wid='default';external_path=None
 if args.external:
  disk=root/'disk';disk.mkdir();made=ok(201,req('/api/workspaces/create-external',{'capability':cap('export',disk),'name':'Test external','protected':args.protected,'password':password}));attach={'capability':made['capability']}
  if args.protected:attach.update(password=password,test_protected=True)
  wid=ok(200,req('/api/workspaces/attach',attach))['id'];external_path=next(disk.glob('Atlas-*'))
 elif args.protected:wid=ok(201,req('/api/workspaces',{'name':'Portable test','protected':True,'password':password}))['id']
 obj=ok(201,req('/api/w/'+wid+'/notes',{'title':'A','text':marker}))['id'];assert search(wid,obj);before=count();assert before==1,('initial body encoding count',before)
 if args.image_replace_direct:
  image_a=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=')
  q=Request(base+'/api/w/'+wid+'/files?filename=image.png',data=image_a,method='POST',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
  with urlopen(q,timeout=30) as r: assert r.status in (200,201); image_id=json.load(r)['id']
  assert search(wid,image_id),'initial image missing from search'
 if args.worker_reopen:
  assert args.external and not args.protected and not args.attach_export
  target=external_path;current=target/'.atlas-worker-index/current';before_generation=current.read_text()
  if args.worker_reopen_stale:
   cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,semantic_enabled=False,rag_enabled=False),'PUT'))
   ok(200,req('/api/w/'+wid+'/notes/'+obj,{'title':'A','text':'WORKER_STALE_CHANGE'},'PUT'))
  ok(200,req('/api/workspaces/'+wid+'/close',{}))
  if args.worker_reopen_stale:
   cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,semantic_enabled=True),'PUT'))
  if actions.exists():actions.unlink()
  reopened=ok(200,req('/api/workspaces/attach',{'capability':cap('attach',target),'id':wid}))['id']
  if args.worker_reopen_stale:
   matches=search(reopened,obj);assert 'WORKER_STALE_CHANGE' in json.dumps(matches,ensure_ascii=False),'stale worker index returned obsolete text'
   assert actions.exists() and actions.read_text().strip(), 'changed content did not update worker index'
   assert current.read_text()!=before_generation,'changed content did not publish a fresh index'
   print('PASS reopening stale worker index updates modified content',flush=True)
  else:
   deadline=time.monotonic()+30
   while time.monotonic()<deadline:
    state=ok(200,req('/api/w/'+reopened+'/semantic/status'));assert not state.get('indexing'),('reopening current worker index started indexing',state)
    if state.get('available') and not state.get('message'):break
    time.sleep(.05)
   else:raise AssertionError(('worker index did not become ready',state))
   assert search(reopened,obj),'reopened worker index not searchable';time.sleep(1)
   assert current.read_text()==before_generation,'reopening worker rewrote index'
   assert not actions.exists() or not actions.read_text().strip(), 'reopening current worker index entered rebuild'
   assert count()==before,'reopening current worker index reencoded text'
   print('PASS current worker index reopens without rebuilding',flush=True)
  raise SystemExit(0)
 removed=None
 if args.attach_export:
  removed=ok(201,req('/api/w/'+wid+'/notes',{'title':'Delete','text':'UNIQUE_REMOVED_BODY'}))['id'];assert search(wid,removed)
  stable=ok(201,req('/api/w/'+wid+'/notes',{'title':'Stable','text':'UNIQUE_STABLE_BODY'}))['id'];assert search(wid,stable)
  predeleted=ok(201,req('/api/w/'+wid+'/notes',{'title':'Trash','text':'UNIQUE_TRASHED_BODY'}))['id'];assert search(wid,predeleted);ok(200,req('/api/w/'+wid+'/objects/delete-many',{'ids':[predeleted]}));assert not search(wid,predeleted)
  if not args.protected:
   native=data_root/'.default/.atlas-semantic-native';native_gen=native/(native/'current').read_text();assert (native_gen/'overlay.json').is_file() and (native_gen/'delta.faiss').is_file(), 'test needs a native base+delta overlay'
 if args.stale_direct:
  assert args.attach_export and args.require_direct
  cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,semantic_enabled=False,rag_enabled=False),'PUT'))
  ok(200,req('/api/w/'+wid+'/notes/'+obj,{'title':'A','text':'UNIQUE_AFTER_INDEX_EDIT'},'PUT'))
 dest=root/'exports';dest.mkdir()
 if args.protected and not args.external:ok(200,req('/api/workspaces/'+wid+'/close',{}))
 params={'capability':cap('export',dest),'ids':[wid]}
 if args.protected:params['passwords']={wid:password}
 exp=ok(200,req('/api/workspaces/export',params));folder=pathlib.Path(exp.get('path') or dest/exp['folder']);assert folder.is_dir()
 # Simulate second machine: separate Atlas data root and fresh model settings/process.
 os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15);data_root=root/'destination-data';next_device='metal' if args.from_device=='cpu' else 'cpu';settings['embedding']=next_device;(models/'settings.json').write_text(json.dumps(settings))
 if args.cancel_direct:
  assert args.attach_export and not args.protected
  barrier=root/'native-step-barrier';barrier.mkdir();env['ATLAS_TEST_NATIVE_STEP_BARRIER']=str(barrier)
 if args.io_error_direct:
  assert args.attach_export and not args.protected
  env['ATLAS_TEST_NATIVE_READ_ERROR']='1'
 if args.search_io_error_direct:
  assert args.attach_export and args.require_direct and not args.protected
  env['ATLAS_TEST_NATIVE_SEARCH_ERROR_ONCE']='1'
 proc=subprocess.Popen([args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('destination backend failed',port);base='http://127.0.0.1:'+port
 if args.external or args.attach_export:
  target=external_path if args.external else folder/'workspace-1'
  if args.symlink_direct:
   assert args.attach_export and not args.require_direct
   current=target/'.atlas-semantic-native/current';existing_generation=current.read_text();outside=root/'outside-index-current';outside.write_text(existing_generation);current.unlink();current.symlink_to(outside)
  if args.require_direct and not args.stale_direct and not args.protected:
   index=target/'.atlas-semantic-native'
   before_files={str(path.relative_to(index)):hashlib.sha256(path.read_bytes()).hexdigest() for path in index.rglob('*') if path.is_file()}
  if (args.require_direct or args.symlink_direct or args.io_error_direct) and actions.exists():actions.unlink()
  if args.symlink_direct or args.io_error_direct:encoder_before=log.read_bytes() if log.exists() else b''
  attach={'capability':cap('attach',target)}
  if args.protected:attach.update(password=password,test_protected=True)
  copy=ok(200,req('/api/workspaces/attach',attach))['id']
 else:
  items=ok(201,req('/api/workspaces/import',{'capability':cap('import',folder)}))['workspaces'];assert len(items)==1;copy=items[0]['id']
  if args.protected:ok(200,req('/api/workspaces/'+copy+'/unlock',{'password':password}))
 if args.cancel_direct:
  # Start the validation, pause its first step and verify cancellation itself
  # does not wait for the storage worker to finish that step.
  ok(200,req('/api/w/'+copy+'/semantic/status'));deadline=time.monotonic()+20
  while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.02)
  assert (barrier/'ready').exists(),'native index check did not reach barrier'
  try:
   with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
    future=executor.submit(req,'/api/w/'+copy+'/semantic/cancel',{})
    try:
     cancel=future.result(timeout=1)
    finally:
     (barrier/'go').write_text('go')
   assert cancel[0]==200,('cancel failed',cancel)
  finally:
   (barrier/'go').touch()
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   state=ok(200,req('/api/w/'+copy+'/semantic/status'))
   if state.get('cancelled') and not state.get('indexing') and not state.get('checking'):break
   time.sleep(.05)
  else:raise AssertionError(('cancel did not stop verification',state))
  assert not (target/'.atlas-worker-index').exists(), 'cancelled validation built a second index'
  assert not actions.exists() or not actions.read_text().strip(), 'cancelled validation entered index build'
  ok(202,req('/api/w/'+copy+'/semantic/rebuild',{'force':False}))
  assert search(copy,obj),'resume did not restore search'
  assert not (target/'.atlas-worker-index').exists(), 'resume unnecessarily built a second index'
  assert not actions.exists() or not actions.read_text().strip(), 'resume unnecessarily entered index build'
  print('PASS cancellation and resume verify without rebuilding index',flush=True);raise SystemExit(0)
 if args.symlink_direct or args.io_error_direct:
  deadline=time.monotonic()+20
  while time.monotonic()<deadline:
   state=ok(200,req('/api/w/'+copy+'/semantic/status'))
   assert not state.get('indexing'),('invalid/unavailable index triggered a rebuild',state)
   if not state.get('checking') and not state.get('available') and state.get('message'):break
   time.sleep(.05)
  else:raise AssertionError(('invalid/unavailable index not reported',state))
  code,data=req('/api/w/'+copy+'/search',{'mode':'semantic','query':'test','object_ids':[obj]})
  assert code in (409,503),('invalid/unavailable index was searched',code,data)
  assert not (target/'.atlas-worker-index').exists(), 'invalid/unavailable index caused a second index'
  assert not actions.exists() or not actions.read_text().strip(), 'invalid/unavailable index entered index build'
  assert (log.read_bytes() if log.exists() else b'')==encoder_before, 'invalid/unavailable index caused model inference'
  if args.symlink_direct:assert outside.read_text()==existing_generation,'index check modified a file outside workspace'
  print('PASS invalid/unavailable native index reports error without rebuild or inference',flush=True);raise SystemExit(0)
 if args.require_direct:
  assert args.attach_export
  if not args.stale_direct:
   # Loading the query encoder or validating a portable marker may take time;
   # it must not start the index-building path.
   deadline=time.monotonic()+30
   while time.monotonic()<deadline:
    state=ok(200,req('/api/w/'+copy+'/semantic/status'))
    assert not state.get('indexing'), ('unnecessary indexing after attach',state)
    if state.get('available') and not state.get('message'):break
    time.sleep(.1)
   else:raise AssertionError(('portable index did not become ready',state))
  result=req('/api/w/'+copy+'/search',{'mode':'semantic','query':'тестовый вопрос','object_ids':[obj]})
  if args.search_io_error_direct:
   assert result[0] in (409,503),('search read error not reported',result)
   time.sleep(1)
   assert not (target/'.atlas-worker-index').exists(), 'search read error built a second index'
   assert not actions.exists() or not actions.read_text().strip(), 'search read error entered index build'
   assert count()==before, 'search read error reencoded source text'
   retry=req('/api/w/'+copy+'/search',{'mode':'semantic','query':'тестовый вопрос','object_ids':[obj]})
   assert retry[0]==200 and retry[1].get('results'),('search did not recover after temporary read error',retry)
   time.sleep(1)
   assert not (target/'.atlas-worker-index').exists(), 'recovery built a second index'
   assert not actions.exists() or not actions.read_text().strip(), 'recovery entered index build'
   assert count()==before, 'recovery reencoded source text'
   print('PASS search read error recovers without index rebuild',flush=True);raise SystemExit(0)
  if args.stale_direct:
   if result[0]==200: assert 'UNIQUE_AFTER_INDEX_EDIT' in json.dumps(result[1],ensure_ascii=False), ('stale result immediately after attach',result)
   else: assert result[0]==503, result
   matches=search(copy,obj);assert 'UNIQUE_AFTER_INDEX_EDIT' in json.dumps(matches,ensure_ascii=False), ('stale result after processing',matches)
   print('PASS updated document is not lost',flush=True)
  else:
   assert result[0]==200,('unnecessary blocking index preparation after attach',result)
   assert result[1]['results'], 'direct portable search returned no results'
   time.sleep(1)
   assert not (target/'.atlas-worker-index').exists(), 'portable search built a second index'
   assert not actions.exists() or not actions.read_text().strip(), ('portable search traversed documents to build an index', actions.read_text())
   if not args.protected:
    after_files={str(path.relative_to(index)):hashlib.sha256(path.read_bytes()).hexdigest() for path in index.rglob('*') if path.is_file()}
    assert before_files==after_files, 'portable search modified the transferred index'
   assert count()==before,('portable search reencoded unchanged text',before,count())
   print('PASS immediate portable search without creating a second index',flush=True)
   if args.image_replace_direct:
    image_b=image_a+b'REPLACED_IMAGE_BYTES'
    q=Request(base+'/api/w/'+copy+'/files/'+image_id+'?filename=image.png',data=image_b,method='PUT',headers={'X-Atlas-Token':token,'Content-Type':'application/octet-stream'})
    with urlopen(q,timeout=30) as r: assert r.status in (200,201)
    assert search(copy,image_id),'replaced image missing from search'
    encoded=base64.b64encode(image_b).decode()
    assert any(encoded in line for line in log.read_text().splitlines()),'replacement image was not encoded'
    print('PASS image replacement updates the direct index',flush=True)
  raise SystemExit(0)
 assert search(copy,obj);time.sleep(1);after=count();assert after==before,('different machine/device reencoded identical model',before,after);print('PASS import into new device reuses vectors',flush=True)
 if args.attach_export:
  assert not search(copy,predeleted), 'deleted note appeared after transfer'
  before_restore=sum('UNIQUE_TRASHED_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines());ok(200,req('/api/w/'+copy+'/trash/restore-many',{'ids':[predeleted]}));assert search(copy,predeleted);after_restore=sum('UNIQUE_TRASHED_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines());assert after_restore==before_restore,('restored note reencoded after transfer',before_restore,after_restore)
 if args.corrupt_worker:
  assert args.attach_export, '--corrupt-worker requires --attach-export'
  changed='CHANGED_BEFORE_CORRUPTION';ok(200,req('/api/w/'+copy+'/notes/'+obj,{'title':'A','text':changed},'PUT'));assert search(copy,obj);ok(200,req('/api/w/'+copy+'/objects/delete-many',{'ids':[removed]}));assert not search(copy,removed)
  out=root/'corrupt-return';out.mkdir();exp=ok(200,req('/api/workspaces/export',{'capability':cap('export',out),'ids':[copy]}));return_folder=pathlib.Path(exp.get('path') or out/exp['folder'])
  os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15);data_root=root/'corrupt-data'
  def restart():
   global proc,base
   proc=subprocess.Popen([args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('backend restart failed',port);base='http://127.0.0.1:'+port
  restart();items=ok(201,req('/api/workspaces/import',{'capability':cap('import',return_folder)}))['workspaces'];assert len(items)==1;local=items[0]['id']
  os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15)
  workers=list(data_root.rglob('.atlas-worker-index/current'));assert len(workers)==1,workers;worker=workers[0].parent;generation=(worker/'current').read_text();active=worker/generation;checksum=active/('overlay.sha256' if (active/'overlay.json').exists() else 'metadata.sha256');assert checksum.is_file(),checksum;checksum.write_text('corrupt')
  stable_before=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
  restart();matches=search(local,obj);assert matches and changed in json.dumps(matches,ensure_ascii=False),('changed note lost after corrupted worker',matches);assert search(local,stable);assert not search(local,removed)
  stable_after=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines());assert stable_after==stable_before,('corrupt worker caused unchanged note reencoding',stable_before,stable_after)
  reconciled=worker.parent/'.atlas-semantic-native'/'worker-reconciled';assert not reconciled.exists() or reconciled.read_text()!=generation, 'corrupt worker generation was marked reconciled'
  print('PASS corrupt worker recovered using valid native index',flush=True);raise SystemExit(0)
 def current_generations():
  if not (args.external or args.attach_export) or args.protected:return None
  workspace=external_path if args.external else folder/'workspace-1'
  return tuple((workspace/name/'current').read_text() if (workspace/name/'current').is_file() else None for name in ('.atlas-semantic-native','.atlas-worker-index'))
 before_generations=current_generations()
 cfg=ok(200,req('/api/settings/app'));next_variant='q8' if args.start_bf16 else 'bf16';initial_variant='bf16' if args.start_bf16 else 'q8';ok(200,req('/api/settings/app',dict(cfg,embedding_variant=next_variant),'PUT'))
 if args.compatible_models:
  state=ok(200,req('/api/w/'+copy+'/semantic/status'));assert not state.get('indexing'),('compatible variant started indexing',state)
 assert search(copy,obj)
 if args.compatible_models:
  assert count()==after,('known model switch reencoded unchanged note',initial_variant,next_variant,after,count())
  cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,embedding_variant=initial_variant),'PUT'));assert search(copy,obj)
  assert count()==after,('known model switch back reencoded unchanged note',next_variant,initial_variant,after,count());assert current_generations()==before_generations,('compatible variant rewrote an unchanged index',before_generations,current_generations());print('PASS known Q8/BF16 switch reuses vectors without rebuilding the index',flush=True)
  # Replacing the temporary link with unknown weights must not inherit compatibility.
  q8.unlink();q8.write_bytes(b'unrelated-model-weights');cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,embedding_variant='bf16'),'PUT'));assert search(copy,obj)
  before_unknown=count();cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,embedding_variant='q8'),'PUT'));assert search(copy,obj);assert count()>before_unknown,('unknown model incorrectly reused vectors',before_unknown,count());print('PASS unknown model forces reindex',flush=True)
 else:
  assert count()>after,('different weight file must rebuild',after,count());print('PASS distinct model weights force reindex',flush=True)
 before_vision=count();cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,image_enabled=True),'PUT'));assert search(copy,obj);assert count()>before_vision,('vision index mode was incorrectly treated as same index',before_vision,count());print('PASS changing vision mode rebuilds as needed',flush=True)
 revised='UNIQUE_PORTABLE_VECTOR_BETA_9472';ok(200,req('/api/w/'+copy+'/notes/'+obj,{'title':'A','text':revised},'PUT'));matches=search(copy,obj);assert matches and revised in json.dumps(matches,ensure_ascii=False),('revised note not searchable',matches);calls=[json.loads(line)['input'] for line in log.read_text().splitlines()];assert sum(revised in v and v.startswith('title: none | text: ') for v in calls)==1,('revised text not encoded once',calls);print('PASS editing imported note updates index',flush=True)
 if args.attach_export:
  ok(200,req('/api/w/'+copy+'/objects/delete-many',{'ids':[removed]}));assert not search(copy,removed)
  before_return=sum(revised in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
  before_stable=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
  returned=root/'return-exports';returned.mkdir();exp=ok(200,req('/api/workspaces/export',{'capability':cap('export',returned),'ids':[copy]}));return_folder=pathlib.Path(exp.get('path') or returned/exp['folder'])
  os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15);data_root=root/'return-data'
  proc=subprocess.Popen([args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('return backend failed',port);base='http://127.0.0.1:'+port
  items=ok(201,req('/api/workspaces/import',{'capability':cap('import',return_folder)}))['workspaces'];assert len(items)==1;local=items[0]['id']
  matches=search(local,obj);assert matches and revised in json.dumps(matches,ensure_ascii=False),('returned note missing revision',matches);assert not search(local,removed),'deleted note returned to search';assert search(local,stable),'unchanged note missing after return'
  after_return=sum(revised in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines());assert after_return==before_return,('return reencoded unchanged revised text',before_return,after_return)
  after_stable=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines());assert after_stable==before_stable,('return reencoded unchanged note',before_stable,after_stable)
  print('PASS return to local keeps updated index and deletions without reencoding',flush=True)

finally:
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait()
 shutil.rmtree(root)
