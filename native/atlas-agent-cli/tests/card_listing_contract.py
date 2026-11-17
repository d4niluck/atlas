import json, os, pathlib, socket, subprocess, sys, tempfile, threading

binary = str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix='atlas-list-', dir='/tmp') as tmp:
    root = pathlib.Path(tmp); sockdir = root / 'socket'; sockdir.mkdir(mode=0o700)
    path = sockdir / 'agent.sock'; assert len(os.fsencode(path)) < 100
    server = socket.socket(socket.AF_UNIX); server.bind(str(path)); path.chmod(0o600); server.listen(); server.settimeout(.2)
    key = root / 'key'; key.write_text('test-key'); key.chmod(0o600)
    env = dict(os.environ, ATLAS_TEST_AGENT_SOCKET_DIR=str(sockdir)); stop = threading.Event(); requests = []
    deny = threading.Event()
    cards = [{'id': 'a1', 'title': 'Alpha', 'kind': 'note'}, {'id': 'b2', 'title': 'Beta.pdf', 'kind': 'file'}, {'id': 'c3', 'title': 'Gamma', 'kind': 'note'}]
    def worker():
        while not stop.is_set():
            try: conn, _ = server.accept()
            except socket.timeout: continue
            except OSError: break
            with conn:
                data = b''
                while b'\r\n\r\n' not in data:
                    chunk = conn.recv(4096)
                    if not chunk: break
                    data += chunk
                if not data: continue
                head, body = data.split(b'\r\n\r\n', 1); lines = head.decode().split('\r\n'); method, route, _ = lines[0].split(' ')
                length = int(next((x.split(':',1)[1] for x in lines[1:] if x.lower().startswith('content-length:')), '0'))
                while len(body) < length: body += conn.recv(4096)
                payload = json.loads(body or b'{}'); requests.append((method, route, payload))
                if deny.is_set(): status = 403; result = {'error': 'Forbidden'}
                elif route.endswith('/objects/list'):
                    status = 200; query = payload.get('query'); matched = [v for v in cards if query is None or query.lower() in v['title'].lower()]
                    offset = payload.get('offset', 0); limit = payload.get('limit', 10); page = matched[offset:offset+limit]
                    result = {'items': page, 'total': len(matched), 'next_offset': offset+len(page) if offset+len(page)<len(matched) else None}
                else: status = 404; result = {'error': 'Not found'}
                encoded = json.dumps(result).encode(); conn.sendall(f'HTTP/1.1 {status} Test\r\nContent-Length: {len(encoded)}\r\nConnection: close\r\n\r\n'.encode()+encoded)
    thread = threading.Thread(target=worker, daemon=True); thread.start()
    def run(*args):
        requests.clear(); result = subprocess.run([binary, '--key-file', str(key), *args], env=env, capture_output=True, text=True, timeout=10)
        return result, list(requests)
    failures = []
    def case(name, fn):
        try: fn(); print('PASS', name, flush=True)
        except Exception as e: failures.append((name, str(e))); print('FAIL', name, str(e), flush=True)
    def list_first():
        result, seen = run('--limit', '2', 'list', 'default')
        assert result.returncode == 0, result.stderr
        assert seen == [('POST', '/api/agent/default/objects/list', {'offset': 0, 'limit': 2, 'query': None})], seen
        assert 'Alpha' in result.stdout and 'a1' in result.stdout and 'Beta.pdf' in result.stdout and 'Gamma' not in result.stdout, result.stdout
        assert '2' in result.stdout, result.stdout
    def list_next_json():
        result, seen = run('--json', '--offset', '2', '--limit', '2', 'list', 'default')
        assert result.returncode == 0, result.stderr
        assert seen == [('POST', '/api/agent/default/objects/list', {'offset': 2, 'limit': 2, 'query': None})], seen
        obj = json.loads(result.stdout); assert [c['id'] for c in obj['items']] == ['c3'] and obj['total'] == 3 and obj['next_offset'] is None, obj
    def search_titles():
        result, seen = run('--json', '--limit', '1', 'search-titles', 'default', 'beta')
        assert result.returncode == 0, result.stderr
        assert seen == [('POST', '/api/agent/default/objects/list', {'offset': 0, 'limit': 1, 'query': 'beta'})], seen
        obj = json.loads(result.stdout); assert [c['id'] for c in obj['items']] == ['b2'] and obj['total'] == 1, obj
    def refusal():
        deny.set()
        try:
            result, seen = run('list', 'default')
            assert result.returncode != 0 and result.stderr.strip(), (result, seen)
            assert seen and seen[0][1] == '/api/agent/default/objects/list', seen
        finally: deny.clear()
    case('human list paginates', list_first); case('json next page', list_next_json); case('title search uses query', search_titles); case('forbidden is reported', refusal)
    stop.set(); server.close(); thread.join(timeout=2)
    if failures: print('FAILURES:', len(failures)); sys.exit(1)
    print('Card listing CLI contract passed.')
