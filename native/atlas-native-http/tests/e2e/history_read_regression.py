#!/usr/bin/env python3
"""Isolated history regression; use --old-backend to test an upgrade.

Example: python3 history_read_regression.py --backend /path/to/backend \\
  --old-backend /path/to/previous/backend --resources /path/to/dylibs
Add --history-rows 54 --text-repeats 70000 for a large timing probe.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import time
from urllib.error import HTTPError
from urllib.request import ProxyHandler, Request, build_opener, install_opener, urlopen

parser = argparse.ArgumentParser()
parser.add_argument('--backend', required=True)
parser.add_argument('--old-backend')
parser.add_argument('--resources', required=True)
parser.add_argument('--history-rows', type=int, default=2)
parser.add_argument('--text-repeats', type=int, default=7000)
args = parser.parse_args()
if not 1 <= args.history_rows <= 100 or args.text_repeats < 30:
    parser.error('history rows must be 1..100 and text repeats must be at least 30')
install_opener(build_opener(ProxyHandler({})))
proc = None
base = ''
token = 'isolated-history-regression'

with tempfile.TemporaryDirectory(prefix='atlas-history-regression-') as tmp:
    root = Path(tmp)
    models = root / 'models'
    models.mkdir()
    (models / 'embeddinggemma-2-q8.gguf').write_bytes(b'fake-encoder')
    (models / 'gemma-4-e2b-it-q4.gguf').write_bytes(b'fake-generator')
    (models / 'settings.json').write_text(json.dumps({'embedding_variant': 'q8', 'embedding': 'cpu', 'llm': 'cpu', 'semantic_enabled': True, 'rag_enabled': True, 'lexical_enabled': True, 'image_enabled': False, 'agentic': False, 'idle_enabled': True, 'idle_minutes': 15, 'model': 'e2b'}))
    env = os.environ.copy()
    env.update({'NO_PROXY': '*', 'DYLD_LIBRARY_PATH': args.resources, 'LD_LIBRARY_PATH': args.resources})

    def start(binary):
        global proc, base
        proc = subprocess.Popen([binary, '--root', str(root / 'data'), '--port', '0', '--token-stdin', '--model-path', str(models / 'embeddinggemma-2-q8.gguf'), '--rag-model-dir', str(models), '--llama-server', str(Path(__file__).with_name('mock-search-model-server.py'))], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, start_new_session=True, env=env)
        proc.stdin.write(token + '\n')
        proc.stdin.flush()
        port = proc.stdout.readline().strip()
        assert port.isdigit(), ('backend did not start', port)
        base = 'http://127.0.0.1:' + port

    def stop():
        if proc and proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait(timeout=10)

    def request(path, data=None, method=None):
        payload = None if data is None else json.dumps(data, ensure_ascii=False).encode()
        req = Request(base + path, data=payload, method=method, headers={'X-Atlas-Token': token, 'Content-Type': 'application/json'})
        try:
            with urlopen(req, timeout=180) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.loads(error.read())

    def ok(result, expected=200):
        assert result[0] == expected, result
        return result[1]

    def history():
        return ok(request('/api/w/default/history'))['items']

    def check_valid(items):
        flags = [(r.get('stale'), r['chunks'][0].get('stale')) for entry in items for r in entry['results']]
        assert len(flags) == args.history_rows * 128 and not any(a or b for a, b in flags), flags[:5]

    try:
        start(args.old_backend or args.backend)
        text = 'машинное обучение локальный поиск ' + ('indexing data retrieval ' * args.text_repeats)
        note = ok(request('/api/w/default/notes', {'title': 'Большая книга', 'text': text}), 201)['id']
        databases = list((root / 'data').rglob('atlas.sqlite3'))
        assert len(databases) == 1, databases
        database = databases[0]
        source_hash = hashlib.sha256(json.dumps(['Большая книга', text], ensure_ascii=False).encode()).hexdigest()
        results = [{'id': note, 'title': 'Большая книга', 'score': 0.9, '_source_hash': source_hash, 'chunks': [{'field': 'body', 'start': i, 'end': i + 500, 'text': text[i:i + 500]}]} for i in range(128)]
        with sqlite3.connect(database) as db:
            for n in range(args.history_rows):
                db.execute('INSERT INTO search_history(query,mode,results,created_at,scope_known,answer,sources) VALUES(?,?,?,?,?,?,?)', ('question' + str(n), 'rag', json.dumps(results), time.time(), 1, 'answer', '[]'))
        baseline_started = time.monotonic()
        baseline = history()
        baseline_seconds = time.monotonic() - baseline_started
        check_valid(baseline)
        if args.old_backend:
            stop()
            snapshot = root / 'old-snapshot.sqlite'
            with sqlite3.connect(database) as original, sqlite3.connect(snapshot) as copy:
                original.backup(copy)
            with sqlite3.connect(snapshot) as copy:
                assert copy.execute('SELECT count(*) FROM search_history').fetchone()[0] == args.history_rows
            start(args.backend)
        start_new = time.monotonic()
        after = history()
        new_seconds = time.monotonic() - start_new
        assert after == baseline, 'upgrade changed saved history or source freshness'
        stop()
        start(args.backend)
        assert history() == baseline, 'history changed after restart'
        ok(request('/api/w/default/notes/' + note, {'title': 'Большая книга', 'text': 'новый текст'}, 'PUT'))
        changed = history()
        flags = [(r.get('stale'), r['chunks'][0].get('stale')) for entry in changed for r in entry['results']]
        assert len(flags) == args.history_rows * 128 and all(a and b for a, b in flags), flags[:5]
        for index, corrupt in enumerate([[42], [{'id': note, 'chunks': [42]}], [{'id': note, 'chunks': []}]]):
            with sqlite3.connect(database) as db:
                db.execute('DELETE FROM search_history')
                db.execute('INSERT INTO search_history(query,mode,results,created_at,scope_known,answer,sources) VALUES(?,?,?,?,?,?,?)', ('malformed', 'rag', json.dumps(corrupt), time.time(), 1, 'answer', '[42]' if index == 2 else '[]'))
            status, _ = request('/api/w/default/history')
            assert status >= 400, (index, status)
            with sqlite3.connect(database) as db:
                db.execute('DELETE FROM search_history')
            assert history() == [] and proc.poll() is None, 'backend failed to recover from malformed history'
        print(json.dumps({'history_rows': args.history_rows, 'chunks_per_row': 128, 'body_bytes': len(text.encode()), 'old_backend_seconds': baseline_seconds if args.old_backend else None, 'new_backend_seconds': new_seconds, 'valid_chunks': args.history_rows * 128, 'upgrade_tested': bool(args.old_backend), 'restart': 'PASS', 'stale_detection': 'PASS', 'malformed_recovery': 'PASS'}), flush=True)
    finally:
        stop()
