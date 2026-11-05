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
    if mode == 'transfer':
        default_note = expect(201, call('/api/w/default/notes', {'title': 'Первое', 'text': 'Текст первого 57297'}))['id']
        other = expect(201, call('/api/workspaces', {'name': 'Тестовое второе'}))['id']
        note = expect(201, call('/api/w/' + other + '/notes', {'title': 'Перенос', 'text': 'Сохранённый текст 57297'}))['id']
        payload = b'synthetic-file-57297' * 100
        file_id = expect(200, call('/api/w/' + other + '/files?filename=example.bin', raw=payload))['id']
        sentry = dest / 'existing.txt'
        sentry.write_bytes(b'leave unchanged')
        token_once = cap('export', dest)
        response = expect(200, call('/api/workspaces/export', {'capability': token_once, 'ids': [other]}))
        expect(403, call('/api/workspaces/export', {'capability': token_once, 'ids': [other]}))
        exported = Path(response.get('path') or dest / response['folder'])
        assert exported.is_dir() and sentry.read_bytes() == b'leave unchanged'
        imported = expect(201, call('/api/workspaces/import', {'capability': cap('import', exported)}))['workspaces']
        assert len(imported) == 1 and imported[0]['id'] != other, imported
        copied = imported[0]['id']
        assert expect(200, call('/api/w/' + copied + '/notes/' + note))['text'] == 'Сохранённый текст 57297'
        req = Request(url + '/api/w/' + copied + '/files/' + file_id + '/content', headers={'X-Atlas-Token': token})
        with urlopen(req, timeout=10) as response:
            assert response.read() == payload
        assert expect(200, call('/api/w/' + other + '/notes/' + note))['text'] == 'Сохранённый текст 57297'
        response = expect(200, call('/api/workspaces/export', {'capability': cap('export', dest), 'ids': ['default', other]}))
        bundle = Path(response.get('path') or dest / response['folder'])
        imported_all = expect(201, call('/api/workspaces/import', {'capability': cap('import', bundle)}))['workspaces']
        assert len(imported_all) == 2 and len({item['id'] for item in imported_all}) == 2, imported_all
        assert sentry.read_bytes() == b'leave unchanged'
        invalid = dest / 'not-a-workspace'
        invalid.mkdir()
        status, _ = call('/api/workspaces/import', {'capability': cap('import', invalid)})
        assert status >= 400, status
        assert sentry.read_bytes() == b'leave unchanged'
        stop()
        start()
        assert expect(200, call('/api/w/' + copied + '/notes/' + note))['text'] == 'Сохранённый текст 57297'
        for item in imported_all:
            if call('/api/w/' + item['id'] + '/notes/' + default_note)[0] == 200:
                assert expect(200, call('/api/w/' + item['id'] + '/notes/' + default_note))['text'] == 'Текст первого 57297'
                break
        else:
            raise AssertionError('imported set did not preserve the first workspace')
        assert expect(200, call('/api/w/' + other + '/notes/' + note))['text'] == 'Сохранённый текст 57297'
        print('PASS: single and combined snapshot/import, exact file bytes, restart and intact source')
    elif mode == 'password':
        secret = 'synthetic-secret-57297'
        password = 'synthetic-password-57297'
        protected = expect(201, call('/api/workspaces', {'name': 'synthetic-name-57297', 'protected': True, 'password': password}))['id']
        note = expect(201, call('/api/w/' + protected + '/notes', {'title': 'synthetic-title-57297', 'text': secret}))['id']
        payload = b'synthetic-attachment-secret-57297' * 100
        file_id = expect(200, call('/api/w/' + protected + '/files?filename=synthetic-file-57297.bin', raw=payload))['id']
        expect(200, call('/api/workspaces/' + protected + '/close', {}))
        expect(403, call('/api/workspaces/' + protected + '/unlock', {'password': 'wrong'}))
        bad_status, _ = call('/api/workspaces/export', {'capability': cap('export', dest), 'ids': [protected], 'passwords': {protected: 'wrong'}})
        assert bad_status == 403, bad_status
        exported = expect(200, call('/api/workspaces/export', {'capability': cap('export', dest), 'ids': [protected], 'passwords': {protected: password}}))
        folder = Path(exported.get('path') or dest / exported['folder'])
        imported = expect(201, call('/api/workspaces/import', {'capability': cap('import', folder)}))['workspaces']
        assert len(imported) == 1 and imported[0]['id'] != protected, imported
        copied = imported[0]['id']
        listing = expect(200, call('/api/workspaces'))['workspaces']
        copy_name = next(w['name'] for w in listing if w['id'] == copied)
        assert copy_name == 'synthetic-name-57297 копия 1', copy_name
        expect(403, call('/api/workspaces/' + copied + '/unlock', {'password': 'wrong'}))
        expect(200, call('/api/workspaces/' + copied + '/unlock', {'password': password}))
        assert next(w['name'] for w in expect(200, call('/api/workspaces'))['workspaces'] if w['id'] == copied) == copy_name
        assert expect(200, call('/api/w/' + copied + '/notes/' + note))['text'] == secret
        req = Request(url + '/api/w/' + copied + '/files/' + file_id + '/content', headers={'X-Atlas-Token': token})
        with urlopen(req, timeout=10) as response:
            assert response.read() == payload
        expect(200, call('/api/workspaces/' + copied + '/close', {}))
        ordinary_note = expect(201, call('/api/w/default/notes', {'title': 'ordinary', 'text': 'ordinary content 57297'}))['id']
        exported_all = expect(200, call('/api/workspaces/export', {'capability': cap('export', dest), 'ids': ['default', protected], 'passwords': {protected: password}}))
        bundle_all = Path(exported_all.get('path') or dest / exported_all['folder'])
        imported_all = expect(201, call('/api/workspaces/import', {'capability': cap('import', bundle_all)}))['workspaces']
        assert len(imported_all) == 2, imported_all
        identified = set()
        for item in imported_all:
            new_id = item['id']
            if call('/api/w/' + new_id + '/notes/' + ordinary_note)[0] == 200:
                identified.add('ordinary')
            elif call('/api/workspaces/' + new_id + '/unlock', {'password': password})[0] == 200:
                assert expect(200, call('/api/w/' + new_id + '/notes/' + note))['text'] == secret
                expect(200, call('/api/workspaces/' + new_id + '/close', {}))
                identified.add('protected')
        assert identified == {'ordinary', 'protected'}, identified
        stop()
        for directory, _, files in os.walk(base):
            for name in files:
                data = (Path(directory) / name).read_bytes()
                for text in (secret, password, 'synthetic-title-57297', 'synthetic-attachment-secret-57297', 'synthetic-file-57297.bin'):
                    assert text.encode() not in data, (directory, name, text)
        start()
        assert next(w['name'] for w in expect(200, call('/api/workspaces'))['workspaces'] if w['id'] == copied) == copy_name
        expect(200, call('/api/workspaces/' + copied + '/unlock', {'password': password}))
        assert next(w['name'] for w in expect(200, call('/api/workspaces'))['workspaces'] if w['id'] == copied) == copy_name
        expect(200, call('/api/workspaces/' + protected + '/unlock', {'password': password}))
        assert expect(200, call('/api/w/' + protected + '/notes/' + note))['text'] == secret
        req = Request(url + '/api/w/' + protected + '/files/' + file_id + '/content', headers={'X-Atlas-Token': token})
        with urlopen(req, timeout=10) as response:
            assert response.read() == payload
        print('PASS: protected export/import, password checks, note/file restart and plaintext marker check')
    else:
        raise ValueError('mode must be transfer or password')
finally:
    stop()
    shutil.rmtree(base)
