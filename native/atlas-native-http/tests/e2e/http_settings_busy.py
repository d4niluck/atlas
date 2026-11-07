#!/usr/bin/env python3
"""Settings reads remain responsive while an unrelated export holds the workspace lock."""
import argparse,json,os,pathlib,signal,subprocess,tempfile,threading,time,shutil
from urllib.request import Request,urlopen,install_opener,build_opener,ProxyHandler
from urllib.error import HTTPError,URLError
install_opener(build_opener(ProxyHandler({})))
p=argparse.ArgumentParser();p.add_argument('--backend',required=True);p.add_argument('--resources',default='');args=p.parse_args()
root=pathlib.Path(tempfile.mkdtemp(prefix='atlas-settings-busy-'));barrier=root/'gate';barrier.mkdir();token='settings-busy-test';env=os.environ.copy();env.update({'NO_PROXY':'*','ATLAS_TEST_SETTINGS_SERIAL_BARRIER':str(barrier)})
if args.resources:
 for k in ('DYLD_LIBRARY_PATH','LD_LIBRARY_PATH'):env[k]=args.resources+os.pathsep+env.get(k,'')
proc=None;thread=None;result=[]
def request(path,data=None,auth=True,timeout=2):
 headers={'Content-Type':'application/json'}
 if auth:headers['X-Atlas-Token']=token
 q=Request(base+path,data=None if data is None else json.dumps(data).encode(),headers=headers)
 try:
  with urlopen(q,timeout=timeout) as r:return r.status,json.load(r)
 except HTTPError as e:return e.code,{}
try:
 proc=subprocess.Popen([args.backend,'--root',str(root/'data'),'--port','0','--token-stdin'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,text=True,start_new_session=True,env=env)
 proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip();assert port.isdigit();base='http://127.0.0.1:'+port
 def export():
  try:result.append(request('/api/test/hold-serial',{},timeout=25))
  except Exception as e:result.append(e)
 thread=threading.Thread(target=export);thread.start();deadline=time.monotonic()+8
 while not (barrier/'ready').exists() and time.monotonic()<deadline:time.sleep(.05)
 assert (barrier/'ready').exists(),'export did not reach barrier'
 try:code,settings=request('/api/settings/app',timeout=1)
 except (TimeoutError,URLError) as e:raise AssertionError('GET settings blocked by unrelated workspace operation') from e
 assert code==200 and 'semantic_enabled' in settings,(code,settings)
 code,_=request('/api/settings/app',auth=False,timeout=1);assert code==403,'settings require token'
 code,_=request('/api/w/default/notes',auth=False,timeout=1);assert code==403,'other endpoints still require token'
 print('PASS settings GET remains responsive and requires authorization',flush=True)
finally:
 (barrier/'resume').write_text('resume')
 if thread:thread.join(timeout=20)
 if proc and proc.poll() is None:
  os.killpg(proc.pid,signal.SIGTERM)
  try:proc.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=10)
 if not thread or not thread.is_alive():shutil.rmtree(root)
