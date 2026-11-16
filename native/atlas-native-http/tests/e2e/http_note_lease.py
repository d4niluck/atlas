import copy, os, json, pathlib, subprocess, sys, tempfile, urllib.request, urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-note-lease-') as tmp:
 p=subprocess.Popen([sys.argv[1],'--root',str(pathlib.Path(tmp)/'data'),'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env={**os.environ,'ATLAS_TEST_REQUIRE_NOTE_LEASE':'1'})
 try:
  p.stdin.write('owner-test-secret\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def req(path,body,secret='owner-test-secret',method=None,lease=None):
   r=urllib.request.Request('http://127.0.0.1:'+port+path,data=json.dumps(body).encode(),headers={'X-Atlas-Token':secret,'Content-Type':'application/json',**({'X-Atlas-Edit-Lease':lease} if lease else {})},method=method)
   try:
    with urllib.request.urlopen(r,timeout=10) as x:return x.status,json.load(x)
   except urllib.error.HTTPError as x:return x.code,json.load(x)
  def creation_checks(prefix):
   status,first=req(prefix+'/notes',{'title':'UI create','text':'Body'});assert status==201 and isinstance(first.get('board'),dict),(status,first)
   first_board=first['board'];assert any(o['id']==first['id'] for o in first_board['objects'])
   status,second=req(prefix+'/notes',{'title':'Concurrent agent','text':'Body'});assert status==201 and any(o['id']==second['id'] for o in second['board']['objects'])
   assert req(prefix+'/board',first_board,method='PUT')[0]==409
   request=urllib.request.Request('http://127.0.0.1:'+port+prefix+'/files?filename=sample.txt',data=b'file content',headers={'X-Atlas-Token':'owner-test-secret','Content-Type':'application/octet-stream'})
   with urllib.request.urlopen(request,timeout=15) as response:upload=json.load(response)
   assert isinstance(upload.get('board'),dict) and any(o['id']==upload['id'] for o in upload['board']['objects']),upload
  def board_checks(prefix,nid):
   creation_checks(prefix)
   request=urllib.request.Request('http://127.0.0.1:'+port+prefix+'/board',headers={'X-Atlas-Token':'owner-test-secret'})
   with urllib.request.urlopen(request,timeout=10) as response:board=json.load(response)
   status,opened=req(prefix+'/board-rescue',{'action':'open'});assert status==200,(status,opened)
   assert req(prefix+'/board-rescue',{'action':'save','session':'f'*64,'board':board})[0]==409
   assert req(prefix+'/board-rescue',{'action':'save','session':opened['session'],'board':board})[0]==200
   status,drafts=req(prefix+'/board-rescue',{'action':'list'});assert status==200 and drafts['drafts'],(status,drafts)
   draft=drafts['drafts'][0];wrong=dict(draft['board']);wrong['unexpected']='changed'
   status,result=req(prefix+'/board-rescue',{'action':'delete','id':draft['id'],'board':wrong});assert status==200 and not result['deleted']
   status,result=req(prefix+'/board-rescue',{'action':'delete','id':draft['id'],'board':draft['board']});assert status==200 and result['deleted']
   obj=next(x for x in board['objects'] if x['id']==nid);obj['x']=obj.get('x',0)+20
   status,created=req(prefix+'/notes',{'title':'Created concurrently','text':'Body'});assert status==201,(status,created)
   status,saved=req(prefix+'/board',board,method='PUT');assert status==409,(status,saved)
   request=urllib.request.Request('http://127.0.0.1:'+port+prefix+'/board',headers={'X-Atlas-Token':'owner-test-secret'})
   with urllib.request.urlopen(request,timeout=10) as response:board=json.load(response)
   assert any(o['id']==created['id'] for o in board['objects'])
   obj=next(x for x in board['objects'] if x['id']==nid);obj['x']=obj.get('x',0)+20
   status,board=req(prefix+'/board',board,method='PUT');assert status==200,(status,board)
   stale=copy.deepcopy(board);changed=copy.deepcopy(board);next(x for x in changed['objects'] if x['id']==created['id'])['x']+=31
   status,board=req(prefix+'/board',changed,method='PUT');assert status==200,(status,board)
   assert req(prefix+'/board',stale,method='PUT')[0]==409
   edge_stale=copy.deepcopy(board);edge_change=copy.deepcopy(board)
   edge_change['edges'].append({'id':'00000000-0000-4000-8000-000000000001','from':nid,'to':created['id']})
   status,board=req(prefix+'/board',edge_change,method='PUT');assert status==200,(status,board)
   assert req(prefix+'/board',edge_stale,method='PUT')[0]==409
   stale=copy.deepcopy(board)
   assert req(prefix+'/objects/'+created['id'],{},method='DELETE')[0]==200
   assert req(prefix+'/board',stale,method='PUT')[0]==409
   request=urllib.request.Request('http://127.0.0.1:'+port+prefix+'/board',headers={'X-Atlas-Token':'owner-test-secret'})
   with urllib.request.urlopen(request,timeout=10) as response:board=json.load(response)
   assert all(o['id']!=created['id'] for o in board['objects'])
   assert req(prefix+'/trash/'+created['id']+'/restore',{})[0]==200
   assert req(prefix+'/objects/'+created['id'],{},method='DELETE')[0]==200
   assert req(prefix+'/trash/restore-many',{'ids':[created['id']]})[0]==200
   assert req(prefix+'/objects/'+created['id'],{},method='DELETE')[0]==200
   status,undo=req(prefix+'/notes',{'title':'Unrelated undo','text':'Body'});assert status==201
   assert req(prefix+'/objects/'+undo['id']+'/undo-create',{'expected_revision':undo['revision']})[0]==200
   request=urllib.request.Request('http://127.0.0.1:'+port+prefix+'/board',headers={'X-Atlas-Token':'owner-test-secret'})
   with urllib.request.urlopen(request,timeout=10) as response:board=json.load(response)
   obj=next(x for x in board['objects'] if x['id']==nid)
   duplicate=dict(obj);duplicate['title']='Duplicate changed';board['objects'].append(duplicate);assert req(prefix+'/board',board,method='PUT')[0]==409;board['objects'].pop()
   obj['title']='Malicious changed';assert req(prefix+'/board',board,method='PUT')[0]==409
   board['objects']=[o for o in board['objects'] if o['id']!=nid];assert req(prefix+'/board',board,method='PUT')[0]==409
  def rescue_roundtrip(note_path,lease,remove=True):
   path=note_path+'/rescue-draft'
   payload={'action':'save','session':'f'*64,'title':'Bad','text':'Bad'}
   assert req(path,payload)[0]==409
   payload={'action':'save','session':lease,'title':'Rescue','text':'rescue-private-content-237'}
   assert req(path,payload)[0]==200
   status,result=req(path,{'action':'get','session':lease});assert status==200 and result['draft']['text']=='rescue-private-content-237',(status,result)
   status,result=req(path,{'action':'list'});assert status==200 and result['drafts'],(status,result)
   draft=next(x for x in result['drafts'] if x['text']=='rescue-private-content-237')
   status,result=req(path,{'action':'delete','id':draft['id'],'title':'wrong','text':draft['text']});assert status==200 and not result['deleted']
   if remove:
    status,result=req(path,{'action':'delete','id':draft['id'],'title':draft['title'],'text':draft['text']});assert status==200 and result['deleted']
  status,note=req('/api/w/default/notes',{'title':'Test','text':'Body'});assert status==201,(status,note)
  path='/api/w/default/notes/'+note['id']+'/edit-lease'
  assert req(path,{'action':'acquire'},'wrong')[0]==403
  import sqlite3
  for _ in range(20):
   code,temporary=req(path,{'action':'acquire'});assert code==200
   assert req(path,{'action':'release','lease':temporary['lease']})[0]==200
  with sqlite3.connect(pathlib.Path(tmp)/'data'/'.default'/'atlas.sqlite3') as db:assert db.execute('SELECT count(*) FROM note_rescue_drafts').fetchone()[0]==0
  status,a=req(path,{'action':'acquire'});assert status==200,(status,a)
  assert req(path,{'action':'acquire'})[0]==409
  assert req('/api/w/default/objects/'+note['id'],{},method='DELETE')[0]==409
  status,unrelated=req('/api/w/default/notes',{'title':'Other','text':'Other'});assert status==201
  assert req('/api/w/default/objects/delete-many',{'ids':[note['id'],unrelated['id']]})[0]==409
  assert req('/api/w/default/objects/'+unrelated['id'],{},method='DELETE')[0]==200
  board_checks('/api/w/default',note['id'])
  assert req('/api/w/default/objects/'+note['id']+'/undo-create',{'expected_revision':note['revision']})[0]==409
  rescue_roundtrip('/api/w/default/notes/'+note['id'],a['lease'],False)
  assert req(path,{'action':'renew','lease':'wrong'})[0]==409
  assert req(path,{'action':'renew','lease':a['lease']})[0]==200
  note_path='/api/w/default/notes/'+note['id']
  assert req(note_path,{'title':'Bad','text':'Bad'},method='PUT')[0]==409
  assert req(note_path,{'title':'Bad','text':'Bad'},method='PUT',lease='wrong')[0]==409
  assert req(note_path+'/draft',{'title':'Draft','text':'unfinished'},method='PUT',lease=a['lease'])[0]==200
  assert req('/api/w/default/note-drafts/recover',{})[0]==409
  assert req(note_path,{'title':'Updated','text':'Saved'},method='PUT',lease=a['lease'])[0]==200
  assert req(note_path+'/draft',{'title':'Draft','text':'unfinished'},method='DELETE',lease=a['lease'])[0]==200
  commit=path.replace('edit-lease','edit-commit')
  assert req(commit,{'lease':'wrong','title':'Bad','text':'Bad'})[0]==409
  assert req(commit,{'lease':a['lease'],'title':'Updated','text':'Saved'})[0]==200
  assert req(path,{'action':'release','lease':a['lease']})[0]==200
  rescue_roundtrip('/api/w/default/notes/'+note['id'],a['lease'])
  import concurrent.futures,threading
  barrier=threading.Barrier(2)
  def race():
   barrier.wait();return req(path,{'action':'acquire'})
  with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
   results=list(pool.map(lambda _:race(),range(2)))
  assert sorted(r[0] for r in results)==[200,409],results
  winner=next(r[1]['lease'] for r in results if r[0]==200)
  assert req(path,{'action':'release','lease':winner})[0]==200
  assert req(commit,{'lease':a['lease'],'title':'Bad','text':'Bad'})[0]==409
  assert req(path,{'action':'release','lease':a['lease']})[0]==409
  status,pw=req('/api/workspaces',{'name':'Protected','protected':True,'password':'test-password'});assert status==201,(status,pw)
  pp='/api/w/'+pw['id'];status,pn=req(pp+'/notes',{'title':'Protected','text':'Before'});assert status==201,(status,pn)
  pp=pp+'/notes/'+pn['id'];status,pl=req(pp+'/edit-lease',{'action':'acquire'});assert status==200,(status,pl)
  assert req('/api/w/'+pw['id']+'/objects/'+pn['id'],{},method='DELETE')[0]==409
  assert req('/api/w/'+pw['id']+'/objects/delete-many',{'ids':[pn['id']]})[0]==409
  board_checks('/api/w/'+pw['id'],pn['id'])
  assert req('/api/w/'+pw['id']+'/objects/'+pn['id']+'/undo-create',{'expected_revision':pn['revision']})[0]==409
  rescue_roundtrip(pp,pl['lease'])
  assert req(pp,{'title':'No','text':'No'},method='PUT')[0]==409
  assert req(pp,{'title':'Yes','text':'After'},method='PUT',lease=pl['lease'])[0]==200
  assert req(pp+'/draft',{'title':'Draft','text':'Recovery'},method='PUT',lease=pl['lease'])[0]==200
  assert req('/api/w/'+pw['id']+'/note-drafts/recover',{})[0]==409
  assert req(pp+'/edit-lease',{'action':'release','lease':pl['lease']})[0]==200
  assert req(pp+'/edit-lease',{'action':'acquire'})[0]==409
  assert req('/api/w/'+pw['id']+'/note-drafts/recover',{})[0]==200
  status,live=req(pp+'/edit-lease',{'action':'acquire'});assert status==200
  assert req('/api/workspaces/'+pw['id']+'/close',{})[0]==200
  assert req('/api/workspaces/'+pw['id']+'/unlock',{'password':'test-password'})[0]==200
  assert req(pp,{'title':'old','text':'old'},method='PUT',lease=live['lease'])[0]==409
  assert req(pp+'/edit-lease',{'action':'acquire'})[0]==200
  disk=pathlib.Path(tmp)/'disk';disk.mkdir()
  status,c=req('/api/workspaces/authorize',{'operation':'export','path':str(disk)});assert status==200
  status,m=req('/api/workspaces/create-external',{'capability':c['capability'],'name':'External','protected':False});assert status==201,(status,m)
  status,w=req('/api/workspaces/attach',{'capability':m['capability']});assert status==200,(status,w)
  wp='/api/w/'+w['id'];status,en=req(wp+'/notes',{'title':'External','text':'Before'});assert status==201,(status,en)
  ep=wp+'/notes/'+en['id'];status,el=req(ep+'/edit-lease',{'action':'acquire'});assert status==200,(status,el)
  assert req(wp+'/objects/'+en['id'],{},method='DELETE')[0]==409
  assert req(wp+'/objects/delete-many',{'ids':[en['id']]})[0]==409
  board_checks(wp,en['id'])
  assert req(wp+'/objects/'+en['id']+'/undo-create',{'expected_revision':en['revision']})[0]==409
  rescue_roundtrip(ep,el['lease'])
  assert req(ep,{'title':'No','text':'No'},method='PUT')[0]==409
  assert req(ep,{'title':'Yes','text':'After'},method='PUT',lease=el['lease'])[0]==200
  assert req(ep+'/draft',{'title':'Draft','text':'Recovery'},method='PUT',lease=el['lease'])[0]==200
  assert req(wp+'/note-drafts/recover',{})[0]==409
  assert req(ep+'/edit-lease',{'action':'release','lease':el['lease']})[0]==200
  assert req(ep+'/edit-lease',{'action':'acquire'})[0]==409
  assert req(wp+'/note-drafts/recover',{})[0]==200
  status,live=req(ep+'/edit-lease',{'action':'acquire'});assert status==200
  assert req('/api/workspaces/'+w['id']+'/close',{})[0]==200
  status,remembered=req('/api/workspaces/remembered-attach',{'id':w['id']});assert status==200,(status,remembered)
  status,again=req('/api/workspaces/attach',{'capability':remembered['capability'],'id':w['id']});assert status==200,(status,again)
  assert req(ep,{'title':'old','text':'old'},method='PUT',lease=live['lease'])[0]==409
  assert req(ep+'/edit-lease',{'action':'acquire'})[0]==200
  disk2=pathlib.Path(tmp)/'disk2';disk2.mkdir()
  status,c=req('/api/workspaces/authorize',{'operation':'export','path':str(disk2)});assert status==200
  status,m=req('/api/workspaces/create-external',{'capability':c['capability'],'name':'External protected','protected':True,'password':'test-password'});assert status==201,(status,m)
  status,w=req('/api/workspaces/attach',{'capability':m['capability'],'password':'test-password','test_protected':True});assert status==200,(status,w)
  xp='/api/w/'+w['id'];status,xn=req(xp+'/notes',{'title':'Protected external','text':'Before'});assert status==201,(status,xn)
  xp=xp+'/notes/'+xn['id'];status,xl=req(xp+'/edit-lease',{'action':'acquire'});assert status==200,(status,xl)
  assert req('/api/w/'+w['id']+'/objects/'+xn['id'],{},method='DELETE')[0]==409
  assert req('/api/w/'+w['id']+'/objects/delete-many',{'ids':[xn['id']]})[0]==409
  board_checks('/api/w/'+w['id'],xn['id'])
  assert req('/api/w/'+w['id']+'/objects/'+xn['id']+'/undo-create',{'expected_revision':xn['revision']})[0]==409
  rescue_roundtrip(xp,xl['lease'])
  assert req(xp,{'title':'No','text':'No'},method='PUT')[0]==409
  assert req(xp,{'title':'Yes','text':'After'},method='PUT',lease=xl['lease'])[0]==200
  assert req(xp+'/draft',{'title':'Draft','text':'Recovery'},method='PUT',lease=xl['lease'])[0]==200
  assert req('/api/w/'+w['id']+'/note-drafts/recover',{})[0]==409
  assert req(xp+'/edit-lease',{'action':'release','lease':xl['lease']})[0]==200
  assert req(xp+'/edit-lease',{'action':'acquire'})[0]==409
  assert req('/api/w/'+w['id']+'/note-drafts/recover',{})[0]==200
  status,live=req(xp+'/edit-lease',{'action':'acquire'});assert status==200
  assert req('/api/workspaces/'+w['id']+'/close',{})[0]==200
  status,remembered=req('/api/workspaces/remembered-attach',{'id':w['id']});assert status==200
  status,again=req('/api/workspaces/attach',{'capability':remembered['capability'],'id':w['id'],'password':'test-password','test_protected':True});assert status==200,(status,again)
  assert req(xp,{'title':'old','text':'old'},method='PUT',lease=live['lease'])[0]==409
  assert req(xp+'/edit-lease',{'action':'acquire'})[0]==200
  status,b=req(path,{'action':'acquire'});assert status==200
  assert req(note_path+'/draft',{'title':'Draft','text':'unfinished'},method='PUT',lease=b['lease'])[0]==200
  assert req(path,{'action':'release','lease':b['lease']})[0]==200
  assert req(path,{'action':'acquire'})[0]==409
  assert req('/api/w/default/note-drafts/recover',{})[0]==200
  status,live=req(path,{'action':'acquire'});assert status==200
  assert req('/api/workspaces/default/close',{})[0]==200
  assert req(note_path,{'title':'old','text':'old'},method='PUT',lease=live['lease'])[0]==409
  assert req(path,{'action':'acquire'})[0]==200
 finally:
  p.terminate();p.wait(timeout=10)
