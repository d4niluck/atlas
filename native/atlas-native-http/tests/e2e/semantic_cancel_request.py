"""The UI must send JSON: an empty POST is invalid; {} reaches cancellation."""
import json, pathlib, subprocess, sys, tempfile, urllib.request, urllib.error
urllib.request.install_opener(urllib.request.build_opener(urllib.request.ProxyHandler({})))
with tempfile.TemporaryDirectory(prefix='atlas-cancel-test-') as tmp:
 p = subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve()), '--root', tmp, '--port', '0'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
 try:
  p.stdin.write('test-token\n'); p.stdin.flush(); port=p.stdout.readline().strip(); assert port.isdigit()
  def post(data):
   req=urllib.request.Request(f'http://127.0.0.1:{port}/api/w/default/semantic/cancel',data=data,headers={'X-Atlas-Token':'test-token','Content-Type':'application/json'},method='POST')
   try:
    with urllib.request.urlopen(req,timeout=10) as resp:return resp.status,json.load(resp)
   except urllib.error.HTTPError as err:return err.code,json.load(err)
  valid=post(b'{}'); empty=post(b'')
  assert valid == (503, {'error':'Модель не загружена'}), valid
  assert empty == (400, {'error':'Invalid request'}), empty
  ui=(pathlib.Path(__file__).resolve().parents[4] / 'ui/src/main.tsx').read_text()
  assert "api('/semantic/cancel',{method:'POST',body:'{}'})" in ui, 'UI cancellation must send JSON object'
  print('UI sends valid JSON; empty POST rejected, valid POST reaches model check')
 finally:
  p.terminate();p.wait(timeout=10)
