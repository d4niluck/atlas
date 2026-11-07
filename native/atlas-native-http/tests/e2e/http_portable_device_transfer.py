#!/usr/bin/env python3
"""Test model identity across portable workspace exports, with synthetic encoder only."""
import argparse, json, os, pathlib, shutil, signal, subprocess, tempfile, time
from urllib.request import Request, urlopen, install_opener, build_opener, ProxyHandler
from urllib.error import HTTPError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--source-backend');p.add_argument('--resources',default='');p.add_argument('--protected',action='store_true');p.add_argument('--external',action='store_true');p.add_argument('--attach-export',action='store_true');p.add_argument('--corrupt-worker',action='store_true');p.add_argument('--from-device',choices=['cpu','metal'],default='cpu');args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-index-independent-'));models=root/'models';models.mkdir();(models/'embeddinggemma-2-q8.gguf').write_bytes(b'Q8-model-v1');(models/'embeddinggemma-2.gguf').write_bytes(b'BF16-model-v1');(models/'embeddinggemma-2-mmproj-q8.gguf').write_bytes(b'fake-mmproj');log=root/'calls.jsonl'
settings={'embedding_variant':'q8','embedding':args.from_device,'llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'};(models/'settings.json').write_text(json.dumps(settings));marker='UNIQUE_PORTABLE_VECTOR_ALPHA_7381';password='test-password-01';token='independent-test-token';env=os.environ.copy();env.update(NO_PROXY='*',ATLAS_TEST_FAKE_ENCODER_LOG=str(log),DYLD_LIBRARY_PATH=args.resources,LD_LIBRARY_PATH=args.resources);proc=None;data_root=root/'data'

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
 proc=subprocess.Popen([args.source_backend or args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-portable-embedding-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('backend failed',port);base='http://127.0.0.1:'+port
 wid='default';external_path=None
 if args.external:
  disk=root/'disk';disk.mkdir();made=ok(201,req('/api/workspaces/create-external',{'capability':cap('export',disk),'name':'Test external','protected':args.protected,'password':password}));attach={'capability':made['capability']}
  if args.protected:attach.update(password=password,test_protected=True)
  wid=ok(200,req('/api/workspaces/attach',attach))['id'];external_path=next(disk.glob('Atlas-*'))
 elif args.protected:wid=ok(201,req('/api/workspaces',{'name':'Portable test','protected':True,'password':password}))['id']
 obj=ok(201,req('/api/w/'+wid+'/notes',{'title':'A','text':marker}))['id'];assert search(wid,obj);before=count();assert before==1,('initial body encoding count',before)
 removed=None
 if args.attach_export:
  removed=ok(201,req('/api/w/'+wid+'/notes',{'title':'Delete','text':'UNIQUE_REMOVED_BODY'}))['id'];assert search(wid,removed)
  stable=ok(201,req('/api/w/'+wid+'/notes',{'title':'Stable','text':'UNIQUE_STABLE_BODY'}))['id'];assert search(wid,stable)
  predeleted=ok(201,req('/api/w/'+wid+'/notes',{'title':'Trash','text':'UNIQUE_TRASHED_BODY'}))['id'];assert search(wid,predeleted);ok(200,req('/api/w/'+wid+'/objects/delete-many',{'ids':[predeleted]}));assert not search(wid,predeleted)
  native=data_root/'.default/.atlas-semantic-native';native_gen=native/(native/'current').read_text();assert (native_gen/'overlay.json').is_file() and (native_gen/'delta.faiss').is_file(), 'test needs a native base+delta overlay'
 dest=root/'exports';dest.mkdir()
 if args.protected and not args.external:ok(200,req('/api/workspaces/'+wid+'/close',{}))
 params={'capability':cap('export',dest),'ids':[wid]}
 if args.protected:params['passwords']={wid:password}
 exp=ok(200,req('/api/workspaces/export',params));folder=pathlib.Path(exp.get('path') or dest/exp['folder']);assert folder.is_dir()
 # Simulate second machine: separate Atlas data root and fresh model settings/process.
 os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15);data_root=root/'destination-data';next_device='metal' if args.from_device=='cpu' else 'cpu';settings['embedding']=next_device;(models/'settings.json').write_text(json.dumps(settings))
 proc=subprocess.Popen([args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-portable-embedding-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('destination backend failed',port);base='http://127.0.0.1:'+port
 if args.external or args.attach_export:
  target=external_path if args.external else folder/'workspace-1'
  attach={'capability':cap('attach',target)}
  if args.protected:attach.update(password=password,test_protected=True)
  copy=ok(200,req('/api/workspaces/attach',attach))['id']
 else:
  items=ok(201,req('/api/workspaces/import',{'capability':cap('import',folder)}))['workspaces'];assert len(items)==1;copy=items[0]['id']
  if args.protected:ok(200,req('/api/workspaces/'+copy+'/unlock',{'password':password}))
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
   proc=subprocess.Popen([args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-portable-embedding-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('backend restart failed',port);base='http://127.0.0.1:'+port
  restart();items=ok(201,req('/api/workspaces/import',{'capability':cap('import',return_folder)}))['workspaces'];assert len(items)==1;local=items[0]['id']
  os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15)
  workers=list(data_root.rglob('.atlas-worker-index/current'));assert len(workers)==1,workers;worker=workers[0].parent;generation=(worker/'current').read_text();active=worker/generation;checksum=active/('overlay.sha256' if (active/'overlay.json').exists() else 'metadata.sha256');assert checksum.is_file(),checksum;checksum.write_text('corrupt')
  stable_before=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
  restart();matches=search(local,obj);assert matches and changed in json.dumps(matches,ensure_ascii=False),('changed note lost after corrupted worker',matches);assert search(local,stable);assert not search(local,removed)
  stable_after=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines());assert stable_after==stable_before,('corrupt worker caused unchanged note reencoding',stable_before,stable_after)
  reconciled=worker.parent/'.atlas-semantic-native'/'worker-reconciled';assert not reconciled.exists() or reconciled.read_text()!=generation, 'corrupt worker generation was marked reconciled'
  print('PASS corrupt worker recovered using valid native index',flush=True);raise SystemExit(0)
 cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,embedding_variant='bf16'),'PUT'));assert search(copy,obj);assert count()>after,('different weight file must rebuild',after,count());print('PASS distinct model weights force reindex',flush=True)
 before_vision=count();cfg=ok(200,req('/api/settings/app'));ok(200,req('/api/settings/app',dict(cfg,image_enabled=True),'PUT'));assert search(copy,obj);assert count()>before_vision,('vision index mode was incorrectly treated as same index',before_vision,count());print('PASS changing vision mode rebuilds as needed',flush=True)
 revised='UNIQUE_PORTABLE_VECTOR_BETA_9472';ok(200,req('/api/w/'+copy+'/notes/'+obj,{'title':'A','text':revised},'PUT'));matches=search(copy,obj);assert matches and revised in json.dumps(matches,ensure_ascii=False),('revised note not searchable',matches);calls=[json.loads(line)['input'] for line in log.read_text().splitlines()];assert sum(revised in v and v.startswith('title: none | text: ') for v in calls)==1,('revised text not encoded once',calls);print('PASS editing imported note updates index',flush=True)
 if args.attach_export:
  ok(200,req('/api/w/'+copy+'/objects/delete-many',{'ids':[removed]}));assert not search(copy,removed)
  before_return=sum(revised in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
  before_stable=sum('UNIQUE_STABLE_BODY' in json.loads(line)['input'] and json.loads(line)['input'].startswith('title: none | text: ') for line in log.read_text().splitlines())
  returned=root/'return-exports';returned.mkdir();exp=ok(200,req('/api/workspaces/export',{'capability':cap('export',returned),'ids':[copy]}));return_folder=pathlib.Path(exp.get('path') or returned/exp['folder'])
  os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=15);data_root=root/'return-data'
  proc=subprocess.Popen([args.backend,'--root',str(data_root),'--port','0','--token-stdin','--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-portable-embedding-server.py'))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env);proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit(),('return backend failed',port);base='http://127.0.0.1:'+port
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
