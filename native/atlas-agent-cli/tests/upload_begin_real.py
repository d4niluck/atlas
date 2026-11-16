import json,os,pathlib,signal,subprocess,sys,tempfile,time,urllib.request
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
backend,cli=sys.argv[1:]
with tempfile.TemporaryDirectory(prefix='atlas-cli-beg-',dir='/private/tmp') as t:
 root=pathlib.Path(t);sockdir=root/'sock';sockdir.mkdir(mode=0o700);disk=root/'disk';disk.mkdir();barrier=root/'barrier';barrier.mkdir()
 env=dict(os.environ,ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir),ATLAS_TEST_AGENT_UPLOAD='1',ATLAS_TEST_REQUIRE_NOTE_LEASE='1',ATLAS_TEST_AGENT_UPLOAD_BEGIN_BARRIER=str(barrier))
 p=subprocess.Popen([backend,'--root',str(root/'data'),'--port','0','--enable-agent-socket'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env,start_new_session=True);pending=None
 try:
  p.stdin.write('owner\n');p.stdin.flush();port=p.stdout.readline().strip();assert port.isdigit(),port
  def owner(path,data=None,method=None):
   b=None if data is None else json.dumps(data).encode();r=urllib.request.Request('http://127.0.0.1:'+port+path,data=b,method=method,headers={'X-Atlas-Token':'owner','Content-Type':'application/json'})
   with urllib.request.urlopen(r,timeout=20) as res:return json.load(res)
  cap=owner('/api/workspaces/authorize',{'operation':'export','path':str(disk)})['capability'];made=owner('/api/workspaces/create-external',{'capability':cap,'name':'Upload','protected':True,'password':'test-password'});wid=owner('/api/workspaces/attach',{'capability':made['capability'],'password':'test-password','test_protected':True})['id']
  a=owner('/api/agents',{'name':'CLI'});key=root/'key';key.write_text(a['token']);key.chmod(0o600);owner('/api/agents/'+a['id'],{'name':'CLI','permissions':{wid:{'add':True}}},'PUT')
  source=root/'data.bin';source.write_bytes(b'cancel before worker')
  command=[cli,'--key-file',str(key),'upload',wid,str(source),'begin-cancel']
  pending=subprocess.Popen(command,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  deadline=time.monotonic()+8
  while not (barrier/'ready').exists() and time.monotonic()<deadline:
   if pending.poll() is not None:raise AssertionError(pending.communicate())
   time.sleep(.02)
  assert (barrier/'ready').exists(),'begin barrier timeout'
  start=time.monotonic();pending.send_signal(signal.SIGINT);out,err=pending.communicate(timeout=5);assert time.monotonic()-start<3 and pending.returncode!=0 and 'Отмена отправлена' in err,(out,err)
  (barrier/'resume').write_text('1');time.sleep(.2)
  replay=subprocess.run(command,env=env,capture_output=True,text=True,timeout=10);assert replay.returncode!=0 and '409' in replay.stderr,(replay.stdout,replay.stderr)
  assert not owner('/api/w/'+wid+'/board')['objects']
  print('CLI cancel during real protected external begin passed')
 finally:
  if pending is not None and pending.poll() is None:pending.kill();pending.wait(timeout=5)
  (barrier/'resume').touch()
  try:os.killpg(p.pid,signal.SIGKILL)
  except ProcessLookupError:pass
  p.wait(timeout=10)
