#!/usr/bin/env python3
import sys,socketserver,http.server,json,os,time,pathlib
path=sys.argv[sys.argv.index('--host')+1];log=os.environ['ATLAS_TEST_FAKE_ENCODER_LOG']
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def reply(self,x):
  body=json.dumps(x,ensure_ascii=False).encode();self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def do_GET(self):self.reply({'status':'ok'})
 def do_POST(self):
  v=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  with open(log,'a',encoding='utf-8') as f:f.write(json.dumps({'path':self.path,**v},ensure_ascii=False)+'\n')
  if self.path=='/v1/embeddings':
   barrier=os.environ.get('ATLAS_QUERY_TEST_BARRIER');text=v.get('input')
   if barrier and isinstance(text,str) and text.startswith('task: search result | query: гонка'):
    pathlib.Path(barrier+'.ready').write_text('ready');deadline=time.monotonic()+30
    while not pathlib.Path(barrier+'.go').exists() and time.monotonic()<deadline:time.sleep(.02)
   self.reply({'data':[{'embedding':[1.0]*768}]})
  elif self.path=='/tokenize':
   barrier=os.environ.get('ATLAS_RAG_TEST_BARRIER')
   if barrier:
    pathlib.Path(barrier+'.ready').write_text('ready');deadline=time.monotonic()+30
    while not pathlib.Path(barrier+'.go').exists() and time.monotonic()<deadline:time.sleep(.02)
   self.reply({'tokens':[1,2]})
  elif self.path=='/v1/chat/completions':
   sysmsg=v['messages'][0]['content'];content=json.dumps({'status':'sufficient'}) if 'Оцени ответ' in sysmsg else json.dumps({'questions':[]}) if 'JSON-объект' in sysmsg else 'Тестовый ответ [1].'
   self.reply({'choices':[{'message':{'content':content}}]})
  else:self.send_error(404)
class S(socketserver.UnixStreamServer):allow_reuse_address=True
with S(path,H) as s:s.serve_forever()
