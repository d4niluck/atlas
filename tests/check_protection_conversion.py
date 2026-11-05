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
    req = Request(url + path, data=body, method='PATCH' if path.endswith('/protection') else ('PUT' if path.endswith('/board') and data is not None else None), headers={'X-Atlas-Token': token, 'Content-Type': 'application/octet-stream' if raw is not None else 'application/json'})
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
    item = expect(201, call('/api/workspaces', {'name': 'Тест копии'}))['id']
    note = expect(201, call('/api/w/' + item + '/notes', {'title': 'маркер', 'text': 'важные тестовые данные'}))['id']
    payload = b'synthetic-data-57297' * 100
    file_id = expect(200, call('/api/w/' + item + '/files?filename=sample.bin', raw=payload))['id']
    if mode == 'conversion':
        prefix = '/api/w/' + item
        second = expect(201, call(prefix + '/notes', {'title': 'Вторая', 'text': 'UniqueKeep57297'}))['id']
        deleted = expect(201, call(prefix + '/notes', {'title': 'Корзина', 'text': 'UniqueTrash57297'}))['id']
        board = expect(200, call(prefix + '/board'))
        board['edges'].append({'id': 'edge-test', 'from': note, 'to': second})
        board['stacks'].append({'id': 'stack-test', 'ids': [note, second], 'title': 'Группа', 'collapsed': False})
        expect(200, call(prefix + '/board', board))
        expect(200, call(prefix + '/objects/delete-many', {'ids': [deleted]}))
        results = expect(200, call(prefix + '/search', {'mode': 'lexical', 'query': 'UniqueKeep57297'}))
        assert second in [r['id'] for r in results['results']]
        history_before = expect(200, call(prefix + '/history'))['items']
        assert any(x['query'] == 'UniqueKeep57297' for x in history_before)
    if mode == 'names':
        exported = expect(200, call('/api/workspaces/export', {'capability': cap('export', dest), 'ids': [item]}))
        folder = Path(exported.get('path') or dest / exported['folder'])
        copies = []
        for i in range(1, 4):
            imported = expect(201, call('/api/workspaces/import', {'capability': cap('import', folder)}))['workspaces']
            copies.append(imported[0])
            assert imported[0]['name'] == f'Тест копии копия {i}', imported[0]
        assert len({c['id'] for c in copies}) == 3
        stop(); start()
        for c in copies:
            assert expect(200, call('/api/w/' + c['id'] + '/notes/' + note))['text'] == 'важные тестовые данные'
    elif mode == 'conversion':
        # Proposed contract; must be adjusted to agreed endpoint before acceptance.
        status, _ = call('/api/workspaces/' + item + '/protection', {'protected': True, 'password': ''})
        assert status >= 400, status
        assert expect(200, call(prefix + '/notes/' + note))['text'] == 'важные тестовые данные'
        expect(200, call('/api/workspaces/' + item + '/protection', {'protected': True, 'password': 'secret-57297'}))
        expect(200, call('/api/workspaces/' + item + '/close', {}))
        for candidate in root.rglob('*'):
            if candidate.is_file():
                raw_data = candidate.read_bytes()
                for secret in ['важные тестовые данные'.encode(), b'synthetic-data-57297', b'UniqueKeep57297', b'UniqueTrash57297', b'secret-57297']:
                    assert secret not in raw_data, ('plaintext remains', str(candidate), secret)
        expect(403, call('/api/workspaces/' + item + '/unlock', {'password': 'wrong'}))
        expect(200, call('/api/workspaces/' + item + '/unlock', {'password': 'secret-57297'}))
        assert expect(200, call('/api/w/' + item + '/notes/' + note))['text'] == 'важные тестовые данные'
        req = Request(url + '/api/w/' + item + '/files/' + file_id + '/content', headers={'X-Atlas-Token': token})
        with urlopen(req, timeout=10) as response: assert response.read() == payload
        board = expect(200, call(prefix + '/board'))
        assert any(e['id'] == 'edge-test' for e in board['edges']), board
        assert any(g['id'] == 'stack-test' for g in board['stacks']), board
        assert any(x['query'] == 'UniqueKeep57297' for x in expect(200, call(prefix + '/history'))['items'])
        assert second in [r['id'] for r in expect(200, call(prefix + '/search', {'mode':'lexical','query':'UniqueKeep57297'}))['results']]
        expect(200, call(prefix + '/trash/restore-many', {'ids':[deleted]}))
        assert expect(200, call(prefix + '/notes/' + deleted))['text'] == 'UniqueTrash57297'
        expect(200, call('/api/workspaces/' + item + '/close', {}))
        status, _ = call('/api/workspaces/' + item + '/protection', {'protected': False, 'password': 'wrong'})
        assert status == 403, status
        expect(200, call('/api/workspaces/' + item + '/protection', {'protected': False, 'password': 'secret-57297'}))
        stop(); start()
        assert expect(200, call('/api/w/' + item + '/notes/' + note))['text'] == 'важные тестовые данные'
        assert expect(200, call(prefix + '/notes/' + deleted))['text'] == 'UniqueTrash57297'
        board = expect(200, call(prefix + '/board'))
        assert any(e['id'] == 'edge-test' for e in board['edges']), board
        assert any(g['id'] == 'stack-test' for g in board['stacks']), board
        assert second in [r['id'] for r in expect(200, call(prefix + '/search', {'mode':'lexical','query':'UniqueKeep57297'}))['results']]
        assert any(x['query'] == 'UniqueKeep57297' for x in expect(200, call(prefix + '/history'))['items'])
    else:
        raise ValueError(mode)
    print('PASS', mode)
finally:
    stop()
    shutil.rmtree(base)
