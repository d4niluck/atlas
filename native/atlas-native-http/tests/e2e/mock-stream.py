#!/usr/bin/env python3
import http.server, json, os, pathlib, socketserver, sys, time

path = sys.argv[sys.argv.index('--host') + 1]
root = pathlib.Path(os.environ['ATLAS_STREAM_FIXTURE'])
counter = 0

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def reply(self, value):
        body = json.dumps(value, ensure_ascii=False).encode()
        self.send_response(200); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
    def do_GET(self): self.reply({'status': 'ok'})
    def do_POST(self):
        global counter
        value = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        if self.path == '/v1/embeddings': return self.reply({'data':[{'embedding':[1.0]*768}]})
        if self.path == '/tokenize': return self.reply({'tokens':[1,2]})
        if self.path != '/v1/chat/completions': return self.send_error(404)
        if not value.get('stream'):
            content = json.dumps({'status':'sufficient'}) if 'Оцени ответ' in value['messages'][0]['content'] else 'Тестовый ответ [1].'
            return self.reply({'choices':[{'message':{'content':content}}]})
        counter += 1; n = str(counter)
        (root / ('started-'+n)).write_text('started')
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        def wait(name):
            deadline=time.monotonic()+180
            while not (root / (name+'-'+n)).exists() and time.monotonic()<deadline: time.sleep(.02)
        def send(text):
            frame='data: '+json.dumps({'choices':[{'delta':{'content':text}}]}, ensure_ascii=False)+'\n\n'
            raw=frame.encode(); nonascii=next((i for i,b in enumerate(raw) if b>=128),None)
            # Force one Cyrillic UTF-8 character to straddle transport writes.
            if nonascii is not None:
                self.wfile.write(raw[:nonascii+1]); self.wfile.flush(); time.sleep(.03); self.wfile.write(raw[nonascii+1:])
            else: self.wfile.write(raw)
            self.wfile.flush()
        try:
            wait('start'); send('Первая '); (root / ('first-'+n)).write_text('sent')
            wait('finish'); send('часть [1].'); self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError): pass
        finally: (root / ('ended-'+n)).write_text('ended')

class Server(socketserver.UnixStreamServer): allow_reuse_address=True
with Server(path,Handler) as server: server.serve_forever()
