#!/usr/bin/env python3
import http.server, json, os, socketserver, sys
if os.environ.get('LLAMA_ARG_CONNECT'):
    raise SystemExit('unexpected inherited remote tunnel setting')
args = sys.argv
socket_path = args[args.index('--host') + 1]
with open(args[args.index('--api-key-file') + 1], encoding='utf-8') as f:
    keys = [f.read().strip()]
if os.environ.get('LLAMA_API_KEY'):
    keys.append(os.environ['LLAMA_API_KEY'])
if os.environ.get('LLAMA_ARG_API_KEY_FILE'):
    with open(os.environ['LLAMA_ARG_API_KEY_FILE'], encoding='utf-8') as f:
        keys.extend(f.read().splitlines())
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b'{}')
    def do_POST(self):
        self.rfile.read(int(self.headers.get('Content-Length', '0')))
        if self.headers.get('Authorization', '').removeprefix('Bearer ') not in keys:
            self.send_response(401); self.end_headers(); self.wfile.write(b'{}'); return
        if self.path == '/tokenize':
            body = json.dumps({'tokens':[1]}).encode()
        elif self.path == '/v1/chat/completions':
            body = json.dumps({'choices':[{'message':{'content':'answer [1]'}}]}).encode()
        else:
            body = json.dumps({'data':[{'embedding':[1] * 512}]}).encode()
        self.send_response(200); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
class Server(socketserver.UnixStreamServer):
    allow_reuse_address = True
with Server(socket_path, Handler) as server:
    with open(os.environ['ATLAS_TEST_SOCKET_MARKER'], 'w', encoding='utf-8') as f:
        f.write(socket_path)
    server.serve_forever()
