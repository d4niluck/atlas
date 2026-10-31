// Run against a disposable workspace: node src-tauri/test-dev-proxy.mjs [PYTHON]
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import assert from 'node:assert/strict';

const here = dirname(fileURLToPath(import.meta.url));
const ui = resolve(here, '..');
const root = resolve(ui, '..');
const workspace = await mkdtemp(join(tmpdir(), 'atlas-dev-test-'));
const token = randomBytes(32).toString('hex');
const backend = spawn(process.argv[2] || 'python3', [resolve(root, 'server.py'), '--workspace', workspace, '--token-stdin'], { stdio: ['pipe', 'ignore', 'inherit'] });
backend.stdin.end(token + '\n');
const vite = spawn(resolve(ui, 'node_modules/.bin/vite'), [], { cwd: ui, env: { ...process.env, ATLAS_DEV_TOKEN: token }, stdio: 'ignore' });
try {
  let ready = false;
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch('http://127.0.0.1:8765/api/board', { headers: { 'X-Atlas-Token': token }, signal: AbortSignal.timeout(500) })).status === 200) { ready = true; break; } } catch {}
    await delay(200);
  }
  assert.ok(ready, 'Backend did not start with this test token');
  ready = false;
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch('http://127.0.0.1:5173/api/board', { signal: AbortSignal.timeout(500) })).status === 200) { ready = true; break; } } catch {}
    await delay(200);
  }
  assert.ok(ready, 'Local proxy did not start; ports 5173 and 8765 must be available');
  for (const [url, headers, expected] of [
    ['http://127.0.0.1:5173/api/board', {}, 200],
    ['http://127.0.0.1:5173/api/board', { Origin: 'http://evil.example' }, 403],
    ['http://127.0.0.1:5173/api/board', { Origin: 'null' }, 403],
    ['http://127.0.0.1:8765/api/board', {}, 403],
    ['http://127.0.0.1:8765/api/board', { 'X-Atlas-Token': 'incorrect' }, 403],
  ]) assert.equal((await fetch(url, { headers })).status, expected);
  console.log('Development proxy checks passed');
} finally {
  backend.kill('SIGTERM');
  vite.kill('SIGTERM');
  await Promise.all([backend, vite].map(child => new Promise(resolveExit => child.exitCode !== null ? resolveExit() : child.once('exit', resolveExit))));
  await rm(workspace, { recursive: true, force: true });
}
