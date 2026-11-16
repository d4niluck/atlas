"""Cancellation must not wait for a workspace operation lock."""
import fcntl,json,pathlib,subprocess,sys,tempfile,threading,urllib.request,urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-cancel-lock-') as tmp:
 p=subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()),'--root',tmp,'--port','0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  p.stdin.write('test-token\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  lock=pathlib.Path(tmp)/'.default'/'.atlas-operation.lock'
  with lock.open('a+b') as f:
   fcntl.flock(f,fcntl.LOCK_EX)
   result=[]
   def cancel(endpoint, method, data):
    req=urllib.request.Request(f'http://127.0.0.1:{port}/api/w/default/semantic/{endpoint}',data=data,headers={'X-Atlas-Token':'test-token','Content-Type':'application/json'},method=method)
    try:
     with urllib.request.urlopen(req,timeout=5) as r:result.append(r.status)
    except urllib.error.HTTPError as e:result.append(e.code)
   threads=[threading.Thread(target=cancel,args=('cancel','POST',b'{}')),threading.Thread(target=cancel,args=('status','GET',None))]
   for t in threads:t.start()
   for t in threads:t.join(1)
   blocked=any(t.is_alive() for t in threads)
   fcntl.flock(f,fcntl.LOCK_UN)
   for t in threads:t.join(6)
   assert not blocked,'cancel/status request waited on workspace lock'
   assert sorted(result)==[200,503],result # no model, but request reached cancellation handler
  print('cancel does not wait on workspace lock')
 finally:
  p.terminate();p.wait(timeout=10)
