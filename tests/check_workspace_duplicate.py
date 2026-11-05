#!/usr/bin/env python3
"""Black-box checks using only artificial workspaces; pass backend binary as argv[1]."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
from urllib.error import HTTPError
from urllib.request import Request, urlopen

binary = os.path.abspath(sys.argv[1])
mode = sys.argv[2] if len(sys.argv) > 2 else 'transfer'
base = Path(tempfile.mkdtemp(prefix='atlas-behavior-'))
root, dest = base / 'registry', base / 'destination'
dest.mkdir()
token = 'atlas-test-only-token'
p = None


def call(path, data=None, raw=None):
    body = raw if raw is not None else (None if data is None else json.dumps(data).encode())
    req = Request(url + path, data=body, headers={'X-Atlas-Token': token, 'Content-Type': 'application/octet-stream' if raw is not None else 'application/json'})
    try:
        with urlopen(req, timeout=30) as response:
            return response.status, json.loads(response.read())
    except HTTPError as exc:
        return exc.code, json.loads(exc.read())


def start():
    global p, url
    p = subprocess.Popen([binary, '--root', str(root), '--port', '0', '--token-stdin'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None, text=True, start_new_session=True)
    p.stdin.write(token + '\n')
    p.stdin.flush()
    port = p.stdout.readline().strip()
    assert port.isdigit(), ('backend did not start', port, p.poll())
    url = 'http://127.0.0.1:' + port


def stop():
    global p
    if p is not None:
        if p.poll() is None:
            os.killpg(p.pid, signal.SIGTERM)
        p.wait(timeout=10)
        p = None


def expect(code, result):
    status, value = result
    assert status == code, (code, status, value)
    return value


def cap(operation, path):
    return expect(200, call('/api/workspaces/authorize', {'operation': operation, 'path': str(path)}))['capability']


try:
 start()
 name='Тест'; item=expect(201,call('/api/workspaces',{'name':name}))['id']; note=expect(201,call('/api/w/'+item+'/notes',{'title':'n','text':'sample'}))['id']
 for i in (1,2):
  clone=expect(201,call('/api/workspaces/'+item+'/duplicate',{}))['workspaces'][0]
  assert clone['name']==f'Тест копия {i}',clone
  assert expect(200,call('/api/w/'+clone['id']+'/notes/'+note))['text']=='sample'
 password='synthetic-password-57297'; protected=expect(201,call('/api/workspaces',{'name':'Секрет','protected':True,'password':password}))['id']
 secret=expect(201,call('/api/w/'+protected+'/notes',{'title':'n','text':'secret body'}))['id']
 expect(200,call('/api/workspaces/'+protected+'/close',{}))
 clone=expect(201,call('/api/workspaces/'+protected+'/duplicate',{}))['workspaces'][0]
 assert clone['name']=='Секрет копия 1',clone
 assert clone['protected'] is True
 expect(403,call('/api/workspaces/'+clone['id']+'/unlock',{'password':'wrong'}))
 expect(200,call('/api/workspaces/'+clone['id']+'/unlock',{'password':password}))
 assert expect(200,call('/api/w/'+clone['id']+'/notes/'+secret))['text']=='secret body'
 expect(200,call('/api/workspaces/'+clone['id']+'/close',{})); stop(); start()
 assert next(w['name'] for w in expect(200,call('/api/workspaces'))['workspaces'] if w['id']==clone['id'])=='Секрет копия 1'
 expect(200,call('/api/workspaces/'+clone['id']+'/unlock',{'password':password}))
 assert expect(200,call('/api/w/'+clone['id']+'/notes/'+secret))['text']=='secret body'
 print('PASS ordinary and protected duplicates, names, passwords and restart')
finally:
 stop(); shutil.rmtree(base)
