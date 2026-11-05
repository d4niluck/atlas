#!/usr/bin/env python3
"""Exercise settings persistence with an isolated backend and no real model."""
import argparse,json,os,pathlib,signal,subprocess,sys,tempfile
from urllib.request import Request,urlopen,build_opener,ProxyHandler,install_opener
from urllib.error import HTTPError

parser=argparse.ArgumentParser();parser.add_argument('--backend',required=True);parser.add_argument('--resources',required=True);args=parser.parse_args()
install_opener(build_opener(ProxyHandler({})))
token='atlas-ngram-settings-test'
with tempfile.TemporaryDirectory(prefix='atlas-ngram-settings-') as tmp:
 root=pathlib.Path(tmp);models=root/'models';models.mkdir();data=root/'data';proc=None;base=''
 fake_server=root/'fake-llama';fake_server.write_text('#!/bin/sh\nif [ "$1" = "--list-devices" ]; then echo "MTL0: fake Metal device"; exit 0; fi\nexit 1\n');fake_server.chmod(0o700)
 def start():
  global proc,base
  env=os.environ.copy();env['DYLD_LIBRARY_PATH']=args.resources;env['LD_LIBRARY_PATH']=args.resources
  proc=subprocess.Popen([args.backend,'--root',str(data),'--port','0','--rag-model-dir',str(models),'--llama-server',str(fake_server)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True,env=env)
  proc.stdin.write(token+'\n');proc.stdin.flush();port=proc.stdout.readline().strip()
  if not port.isdigit():raise AssertionError('Backend failed to start: '+proc.stderr.read(300))
  base='http://127.0.0.1:'+port
 def stop():
  if proc and proc.poll() is None:
   os.killpg(proc.pid,signal.SIGTERM)
   try:proc.wait(timeout=15)
   except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);proc.wait(timeout=10)
 def request(method,payload=None):
  body=None if payload is None else json.dumps(payload).encode()
  q=Request(base+'/api/settings/app',data=body,method=method,headers={'X-Atlas-Token':token,'Content-Type':'application/json'})
  try:
   with urlopen(q,timeout=15) as r:return r.status,json.load(r)
  except HTTPError as e:return e.code,json.load(e)
 def ok(result):
  code,value=result;assert code==200,(code,value);return value
 try:
  start();initial=ok(request('GET'));assert initial.get('ngram_enabled') is False,initial
  legacy=dict(initial);legacy.pop('ngram_enabled');ok(request('PUT',legacy));assert ok(request('GET'))['ngram_enabled'] is False
  before=ok(request('GET'));invalid=dict(before,ngram_enabled='yes');code,_=request('PUT',invalid);assert code==400,code;assert ok(request('GET'))==before
  cpu=dict(ok(request('GET')));cpu.update(llm='cpu',ngram_enabled=True);ok(request('PUT',cpu));assert ok(request('GET'))['ngram_enabled'] is False
  if sys.platform=='darwin':
   metal=dict(ok(request('GET')));metal.update(llm='metal',ngram_enabled=True);ok(request('PUT',metal));assert ok(request('GET'))['ngram_enabled'] is True
   stop();start();saved=ok(request('GET'));assert saved['llm']=='metal' and saved['ngram_enabled'] is True,saved
   legacy=dict(saved);legacy.pop('ngram_enabled');ok(request('PUT',legacy));assert ok(request('GET'))['ngram_enabled'] is True
   cpu=dict(ok(request('GET')));cpu['llm']='cpu';ok(request('PUT',cpu));assert ok(request('GET'))['ngram_enabled'] is False
   stop();start();assert ok(request('GET'))['ngram_enabled'] is False
  print('ngram settings compatibility, normalization and persistence passed')
 finally:stop()
