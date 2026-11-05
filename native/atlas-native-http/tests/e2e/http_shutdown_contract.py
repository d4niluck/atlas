#!/usr/bin/env python3
"""Isolated backend drain test; requires --features test-hook and never touches user data."""
import argparse,json,os,pathlib,select,signal,socket,subprocess,tempfile,threading,time,urllib.request,urllib.error,shutil
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',required=True);args=p.parse_args()
token='isolated-shutdown-contract'; root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-shutdown-contract-')); proc=None; note_id=None

def start(barrier=None,bg=None,control=True,index=None,panic=None,retry=None):
 env=os.environ.copy();env.update({'DYLD_LIBRARY_PATH':args.resources,'LD_LIBRARY_PATH':args.resources,'NO_PROXY':'*'})
 if retry: env["ATLAS_TEST_SHUTDOWN_RETRY_MARKER"]=str(retry)
 if barrier: env['ATLAS_TEST_SHUTDOWN_HTTP_BARRIER']=str(barrier)
 if bg: env['ATLAS_TEST_SHUTDOWN_BG_BARRIER']=str(bg)
 extra=[]
 if index or panic:
  if index: env['ATLAS_TEST_SHUTDOWN_INDEX_BARRIER']=str(index)
  if panic: env['ATLAS_TEST_SHUTDOWN_INDEX_PANIC']=str(panic)
  env['ATLAS_TEST_FAKE_ENCODER_LOG']=str(root/'fake-encoder.log')
  models=root/'models';models.mkdir(exist_ok=True);(models/'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder')
  (models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':False,'lexical_enabled':True,'image_enabled':False,'agentic':False}))
  extra=['--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--rag-model-dir',str(models),'--llama-server',str(pathlib.Path(__file__).with_name('mock-search-model-server.py'))]
 p=subprocess.Popen([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin']+(['--control-stdin'] if control else [])+extra,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True,env=env)
 p.stdin.write(token+'\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit(),('startup failure',port,p.stderr.read());return p,'http://127.0.0.1:'+port

def request(base,path,data=None):
 b=None if data is None else json.dumps(data).encode();q=urllib.request.Request(base+path,data=b,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
 with urllib.request.urlopen(q,timeout=10) as r:return r.status,json.load(r)

def wait_file(path):
 deadline=time.monotonic()+10
 while not path.exists():
  assert time.monotonic()<deadline,('missing marker',str(path));time.sleep(.01)

def assert_no_ack(p):
 assert not select.select([p.stdout],[],[],.5)[0], 'acknowledged before operation ended'
 assert p.poll() is None

def finish(p):
 assert select.select([p.stdout],[],[],10)[0], 'no shutdown response'
 assert p.stdout.readline().strip()=='OK';assert p.wait(timeout=10)==0

def cleanup(p):
 if p and p.poll() is None:
  os.killpg(p.pid,signal.SIGTERM)
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=5)

try:
 # An accepted note write must finish before ACK; late requests must not succeed.
 barrier=root/'http';barrier.mkdir();proc,base=start(barrier=barrier);out={}
 t=threading.Thread(target=lambda:out.setdefault('note',request(base,'/api/w/default/notes',{'title':'drain test','text':'committed'})));t.start();wait_file(barrier/'ready');proc.stdin.write('shutdown\n');proc.stdin.flush();assert_no_ack(proc)
 try:request(base,'/api/w/default/notes',{'title':'late','text':'must not write'});raise AssertionError('late request accepted')
 except (urllib.error.URLError,TimeoutError,ConnectionError):pass
 (barrier/'resume').write_text('resume');t.join(timeout=10);assert not t.is_alive();assert out['note'][0]==201;note_id=out['note'][1]['id'];finish(proc);proc=None
 proc,base=start();status,note=request(base,'/api/w/default/notes/'+note_id);assert status==200 and note['text']=='committed';proc.stdin.write('shutdown\n');proc.stdin.flush();finish(proc);proc=None
 # Background work must finish before ACK even after HTTP request returned.
 bg=root/'background';bg.mkdir();proc,base=start(bg=bg);assert request(base,'/api/test-shutdown-background')[0]==200;wait_file(bg/'ready');proc.stdin.write('shutdown\n');proc.stdin.flush();assert_no_ack(proc);(bg/'resume').write_text('resume');finish(proc);assert (bg/'completed').read_text()=='done';proc=None
 # Protected local vault is closed and can be reopened after shutdown.
 proc,base=start();status,made=request(base,'/api/workspaces',{'name':'protected drain','protected':True,'password':'test-password'});assert status==201;wid=made['id']
 status,note=request(base,'/api/w/'+wid+'/notes',{'title':'protected','text':'secret text'});assert status==201;nid=note['id']
 proc.stdin.write('shutdown\n');proc.stdin.flush();finish(proc);proc=None
 proc,base=start();assert request(base,'/api/workspaces/'+wid+'/unlock',{'password':'test-password'})[0]==200;assert request(base,'/api/w/'+wid+'/notes/'+nid)[1]['text']=='secret text';proc.stdin.write('shutdown\n');proc.stdin.flush();finish(proc);proc=None
 # External workspace worker closes cleanly and releases its session lock.
 disk=root/'disk';disk.mkdir();proc,base=start();cap=request(base,'/api/workspaces/authorize',{'operation':'export','path':str(disk)})[1]['capability'];made=request(base,'/api/workspaces/create-external',{'capability':cap,'name':'external drain','protected':False})[1];wid=request(base,'/api/workspaces/attach',{'capability':made['capability']})[1]['id']
 status,note=request(base,'/api/w/'+wid+'/notes',{'title':'external','text':'external text'});assert status==201;nid=note['id'];proc.stdin.write('shutdown\n');proc.stdin.flush();finish(proc);proc=None
 proc,base=start();source=next(disk.glob('Atlas-*'));cap=request(base,'/api/workspaces/authorize',{'operation':'attach','path':str(source)})[1]['capability'];wid=request(base,'/api/workspaces/attach',{'capability':cap,'id':wid})[1]['id'];assert request(base,'/api/w/'+wid+'/notes/'+nid)[1]['text']=='external text';proc.stdin.write('shutdown\n');proc.stdin.flush();finish(proc);proc=None
 # Closing waits for a partially received external file before shutting its worker down.
 proc,base=start();source=next(disk.glob('Atlas-*'));cap=request(base,'/api/workspaces/authorize',{'operation':'attach','path':str(source)})[1]['capability'];wid=request(base,'/api/workspaces/attach',{'capability':cap,'id':wid})[1]['id']
 port=int(base.rsplit(':',1)[1]);sock=socket.create_connection(('127.0.0.1',port));sock.settimeout(10);data=b'a'*131072
 headers=f'POST /api/w/{wid}/files?filename=drain.bin HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Atlas-Token: {token}\r\nContent-Type: application/octet-stream\r\nContent-Length: {len(data)}\r\nConnection: close\r\n\r\n'.encode();sock.sendall(headers+data[:65536]);time.sleep(.3);proc.stdin.write('shutdown\n');proc.stdin.flush();assert_no_ack(proc);sock.sendall(data[65536:]);response=b''
 while True:
  part=sock.recv(65536)
  if not part: break
  response+=part
 sock.close();assert b'200 OK' in response,response[:300];finish(proc);proc=None
 proc,base=start();cap=request(base,'/api/workspaces/authorize',{'operation':'attach','path':str(source)})[1]['capability'];wid=request(base,'/api/workspaces/attach',{'capability':cap,'id':wid})[1]['id'];board=request(base,'/api/w/'+wid+'/board')[1];obj=next(o for o in board['objects'] if o.get('title')=='drain.bin');download=urllib.request.Request(base+'/api/w/'+wid+'/files/'+obj['id']+'/content',headers={'X-Atlas-Token':token});assert urllib.request.urlopen(download,timeout=10).read()==data;proc.stdin.write('shutdown\n');proc.stdin.flush();finish(proc);proc=None
 # Actual semantic queue work is accounted for until its current iteration completes.
 index=root/'index-barrier';index.mkdir();proc,base=start(index=index)
 status,note=request(base,'/api/w/default/notes',{'title':'index drain','text':'some content requiring index'});assert status==201
 wait_file(index/'ready');status,note2=request(base,'/api/w/default/notes',{'title':'queued index','text':'second queued content'});assert status==201;assert (index/'starts').read_text().count('start')==1;proc.stdin.write('shutdown\n');proc.stdin.flush();assert_no_ack(proc);(index/'resume').write_text('resume');finish(proc);assert (index/'starts').read_text().count('start')>=2, 'queued index work was skipped';proc=None
 # Inspect the published index before any restart can enqueue a rebuild.
 indexdir=root/'data'/'.default'/'.atlas-semantic-native';current=(indexdir/'current').read_text().strip();active=indexdir/current;overlay=json.loads((active/'overlay.json').read_text()) if (active/'overlay.json').exists() else None;base=indexdir/(overlay['base_generation'] if overlay else current);metadata=json.loads((base/'metadata.json').read_text());indexed={r['id'] for r in metadata['records']};indexed.update(r['id'] for r in (overlay or {}).get('delta_metadata',[]));assert {note['id'],note2['id']}.issubset(indexed),('queued documents not published',indexed,note,note2)

 # Recoverable shutdown failure is retried only when requested.
 retry=root/"retry-marker";retry.write_text("fail");proc,base=start(retry=retry);proc.stdin.write("shutdown\n");proc.stdin.flush();assert_no_ack(proc);retry.unlink();finish(proc);proc=None
 # EOF on the control channel also requests a clean shutdown.
 proc,base=start();proc.stdin.close();finish(proc);proc=None
 # A crashed indexing thread must not cause a false successful shutdown ACK.
 marker=root/'panic-ready';proc,base=start(panic=marker);status,_=request(base,'/api/w/default/notes',{'title':'panic case','text':'trigger'});assert status==201;wait_file(marker);time.sleep(.2);proc.stdin.write('shutdown\n');proc.stdin.flush();assert_no_ack(proc);cleanup(proc);proc=None
 # Legacy macOS/Linux launch closes stdin after the token, but must keep serving.
 proc,base=start(control=False);proc.stdin.close();time.sleep(.5);assert proc.poll() is None;assert request(base,'/api/w/default/notes/'+note_id)[1]['text']=='committed';cleanup(proc);proc=None
 print('PASS: accepted write, late request, durable reopen, background drain, protected vault, external worker, real semantic queue, control EOF, legacy stdin')
finally:
 cleanup(proc);shutil.rmtree(root)
