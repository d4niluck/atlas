#!/usr/bin/env python3
import sys,socketserver,http.server,json,hashlib,os
if '--list-devices' in sys.argv:
 print('MTL0: test GPU')
 raise SystemExit(0)
args=sys.argv;path=args[args.index('--host')+1];log=os.environ['ATLAS_TEST_FAKE_ENCODER_LOG']
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  body=b'{"status":"ok"}';self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def do_POST(self):
  value=json.loads(self.rfile.read(int(self.headers['Content-Length'])));text=value['input'];
  with open(log,'a',encoding='utf-8') as f:f.write(json.dumps({'input':text},ensure_ascii=False)+'\n')
  h=hashlib.sha256(text.encode()).digest();out=[(h[i%32]-127.5)/128.0 for i in range(768)];body=json.dumps({'data':[{'embedding':out}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
class S(socketserver.UnixStreamServer):allow_reuse_address=True
with S(path,H) as s:s.serve_forever()
