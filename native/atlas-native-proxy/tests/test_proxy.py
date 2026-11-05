#!/usr/bin/env python3
"""Black-box proxy security checks; provide program path or emulator + program."""
import http.client, http.server, os, re, socketserver, subprocess, sys, tempfile, threading, unittest
from pathlib import Path
CMD=sys.argv[1:]; assert CMD, 'usage: test_proxy.py PROXY [or WINE PROXY]';sys.argv=sys.argv[:1]
BOOTSTRAP='bootstrap-only-1234567890abcdefgh';TOKEN='backend-token-1234567890abcdefghij'
class Backend(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  self.send_response(200 if self.headers.get('X-Atlas-Token')==TOKEN else 403);self.send_header('Content-Length','10');self.end_headers();self.wfile.write(b'backend-ok')
 def log_message(self,*_):pass
class TestProxy(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.tmp=tempfile.TemporaryDirectory(prefix='atlas-proxy-contract-');cls.ui=Path(cls.tmp.name);(cls.ui/'index.html').write_text('test ui',encoding='utf-8')
  cls.backend=socketserver.TCPServer(('127.0.0.1',0),Backend);cls.thread=threading.Thread(target=cls.backend.serve_forever,daemon=True);cls.thread.start()
 @classmethod
 def tearDownClass(cls):cls.backend.shutdown();cls.backend.server_close();cls.thread.join();cls.tmp.cleanup()
 def start(self):
  p=subprocess.Popen(CMD+[str(self.ui),str(self.backend.server_address[1]),'0'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  p.stdin.write(BOOTSTRAP+'\n'+TOKEN+'\n');p.stdin.flush();line=p.stdout.readline().strip()
  if not line.isdigit():
   p.terminate();_,err=p.communicate(timeout=5);self.fail('proxy failed before starting: '+err[:500])
  self.addCleanup(self.stop,p);return p,int(line)
 @staticmethod
 def stop(p):
  if p.poll() is None:p.terminate()
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:p.kill();p.wait()
  for stream in (p.stdin,p.stdout,p.stderr):
   if stream:stream.close()
 def req(self,port,path,headers=None):
  c=http.client.HTTPConnection('127.0.0.1',port,timeout=5)
  try:c.request('GET',path,headers=headers or {});r=c.getresponse();return r.status,dict(r.getheaders()),r.read()
  finally:c.close()
 def test_session_and_origin_isolation(self):
  _,port=self.start()
  status,_,_=self.req(port,'/');self.assertEqual(status,403)
  status,headers,_=self.req(port,'/bootstrap/'+BOOTSTRAP);self.assertEqual(status,303)
  cookie=headers.get('Set-Cookie','');self.assertRegex(cookie,r'^atlas_session=[0-9a-f]{64};');self.assertIn('HttpOnly',cookie);self.assertIn('SameSite=Strict',cookie)
  pair=cookie.split(';',1)[0]
  self.assertEqual(self.req(port,'/bootstrap/'+BOOTSTRAP)[0],403)
  status,_,body=self.req(port,'/',{'Cookie':pair});self.assertEqual((status,body),(200,b'test ui'))
  self.assertEqual(self.req(port,'/',{'Cookie':pair,'Origin':'http://evil.example'})[0],403)
  status,_,body=self.req(port,'/api/check',{'Cookie':pair});self.assertEqual((status,body),(200,b'backend-ok'))
  self.assertNotIn(TOKEN.encode(),body)
 def test_sessions_have_distinct_random_cookies(self):
  _,a=self.start();_,b=self.start();ca=self.req(a,'/bootstrap/'+BOOTSTRAP)[1]['Set-Cookie'];cb=self.req(b,'/bootstrap/'+BOOTSTRAP)[1]['Set-Cookie'];self.assertNotEqual(ca,cb)
if __name__=='__main__':unittest.main(verbosity=2)
