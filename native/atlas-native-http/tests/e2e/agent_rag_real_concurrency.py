import json,os,pathlib,socket,subprocess,sys,tempfile,threading,time,urllib.request,uuid
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-agent-rag-') as t:
 root=pathlib.Path(t).resolve();sockdir=root/'socket';sockdir.mkdir(mode=0o700);production=len(sys.argv)>3 and sys.argv[3]=='production';sockdir=pathlib.Path('/tmp')/f'atlas-agent-{os.getuid()}' if production else sockdir;assert not (sockdir/'agent.sock').exists(), 'Atlas agent socket is already active';models=root/'models';models.mkdir(); source=pathlib.Path(os.environ['ATLAS_TEST_MODEL_DIR']);
 for name in ('embeddinggemma-2-q8.gguf','gemma-4-e2b-it-q4.gguf'):(models/name).symlink_to(source/name)
 (models/'settings.json').write_text(json.dumps({'embedding_variant':'q8','embedding':'cpu','llm':'cpu','semantic_enabled':True,'rag_enabled':True,'lexical_enabled':True,'image_enabled':False,'agentic':False,'idle_enabled':True,'idle_minutes':15,'model':'e2b'}));env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir));mock=pathlib.Path(os.environ['ATLAS_TEST_LLAMA']).resolve();p=subprocess.Popen([sys.argv[1],'--root',str(root/'data'),'--port','0','--enable-agent-socket','--rag-model-dir',str(models),'--model-path',str(models/'embeddinggemma-2-q8.gguf'),'--llama-server',str(mock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit()
  def owner(path,value=None,method=None):
   b=None if value is None else json.dumps(value).encode();r=urllib.request.Request('http://127.0.0.1:'+port+path,data=b,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   with urllib.request.urlopen(r,timeout=10) as v:return v.status,json.load(v)
  def agent(path,key,value=None):
   b=b'' if value is None else json.dumps(value).encode();method='GET' if value is None else 'POST';s=socket.socket(socket.AF_UNIX);s.settimeout(30);s.connect(str(sockdir/'agent.sock'));s.sendall((f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nX-Atlas-Agent-Token: {key}\r\nContent-Type: application/json\r\nContent-Length: {len(b)}\r\n\r\n').encode()+b);out=b''
   while True:
    part=s.recv(65536)
    if not part:break
    out+=part
   s.close();h,b=out.split(b'\r\n\r\n',1);return int(h.split()[1]),json.loads(b)
  for _ in range(1500):
   if (sockdir/'agent.sock').exists():break
   time.sleep(.02)
  assert (sockdir/'agent.sock').exists(), ('agent socket unavailable',p.poll())
  wid='default'; profiles=[owner('/api/agents',{'name':n})[1] for n in ('A','B')]
  for profile in profiles:assert owner('/api/agents/'+profile['id'],{'name':profile['name'],'permissions':{wid:{'read':True,'semantic':True,'smart':True,'lexical':True}}},'PUT')[0]==200
  status,note=owner('/api/w/default/notes',{'title':'Fact','text':'The planet Mars is red due to iron oxide.'});assert status==201
  deadline=time.monotonic()+120
  while True:
   status,value=agent('/api/agent/default/search',profiles[0]['token'],{'query':'Mars','mode':'semantic'})
   if status==200:break
   assert status==503,(status,value)
   assert time.monotonic()<deadline,(status,value)
   time.sleep(.5)
  outcomes=[];start=threading.Barrier(4); requests={profile['id']:str(uuid.uuid4()) for profile in profiles}
  def run_agent(profile):
   start.wait();outcomes.append(('agent',agent('/api/agent/default/rag/search',profile['token'],{'query':'What colour is Mars?', 'request_id':requests[profile['id']]})))
  def run_owner():
   start.wait();outcomes.append(('owner',owner('/api/w/default/rag/search',{'query':'What colour is Mars?'})))
  threads=[threading.Thread(target=run_agent,args=(profile,)) for profile in profiles]+[threading.Thread(target=run_owner)]
  for th in threads:th.start()
  start.wait();time.sleep(2); cancelled=agent('/api/agent/default/rag/cancel/'+requests[profiles[1]['id']],profiles[1]['token'],{}); print('cancel result:',cancelled);samples=[];deadline=time.monotonic()+180
  while any(th.is_alive() for th in threads) and time.monotonic()<deadline:
   result=subprocess.run(['ps','-axo','pid=,ppid=,rss=,%cpu=,command='],capture_output=True,text=True)
   children=[line.strip() for line in result.stdout.splitlines() if len(line.split()) > 4 and line.split()[4] == str(mock) and line.split()[1] == str(p.pid)]
   samples.append(children);
   for line in children:
    if 'embeddinggemma-2-q8.gguf' in line:
     columns=line.split();print('embedding_sample',round(time.monotonic(),2),'pid',columns[0],'ppid',columns[1],'rss_kib',columns[2],'cpu',columns[3],flush=True)
   time.sleep(.5)
  for th in threads:th.join(timeout=1)
  assert all(not th.is_alive() for th in threads), 'requests timed out'
  llm_counts=[sum('gemma-4-e2b-it-q4.gguf' in line for line in sample) for sample in samples]
  embedding_counts=[sum('embeddinggemma-2-q8.gguf' in line for line in sample) for sample in samples]
  print('max E2B processes:',max(llm_counts,default=0),'max embedding processes:',max(embedding_counts,default=0))
  print('peak child CPU percent:',max((sum(float(line.split()[3]) for line in sample) for sample in samples),default=0))
  assert max(llm_counts,default=0)==1, llm_counts
  assert max(embedding_counts,default=0)<=1, embedding_counts
  print('outcomes:',[(name,result[0]) for name,result in outcomes]);print('sampled child-process counts:',sorted(set(len(x) for x in samples)))
  print('peak child RSS KiB:',max((sum(int(line.split()[2]) for line in sample) for sample in samples),default=0))
  assert cancelled[0]==200 and cancelled[1].get('ok') is True,cancelled
  assert len(outcomes)==3 and sorted(result[0] for _,result in outcomes)==[200,200,499],outcomes
 finally:p.terminate();p.wait(timeout=10)
print('real model concurrency completed')
