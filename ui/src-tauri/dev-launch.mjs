// Development launcher: starts isolated local backend and Vite, then Tauri.
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const here = dirname(fileURLToPath(import.meta.url));
const ui = resolve(here, '..');
const root = resolve(ui, '..');
const workspace = process.argv[2];
const backendPath = process.argv[3];
const model = process.argv[4];
const llama = process.argv[5];
if (!workspace || !backendPath || Boolean(model) !== Boolean(llama)) {
  console.error('Usage: node src-tauri/dev-launch.mjs WORKSPACE_ROOT BACKEND [MODEL_PATH LLAMA_SERVER]');
  process.exit(2);
}
const token = randomBytes(32).toString('hex');
const children = [];
function run(command, args, options = {}) {
  const child = spawn(command, args, { stdio: ['pipe', 'inherit', 'inherit'], ...options });
  child.on('error', error => console.error(`Failed to start ${command}: ${error.message}`));
  children.push(child);
  return child;
}
let shuttingDown = false;
function cleanup() {
  if (shuttingDown) return;
  shuttingDown = true;
  for (const child of children) if (!child.killed) child.kill('SIGTERM');
}
process.on('SIGINT', () => { cleanup(); process.exit(130); });
process.on('SIGTERM', () => { cleanup(); process.exit(143); });
async function ready(url, headers = {}) {
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(url, { headers, signal: AbortSignal.timeout(500) })).ok) return; } catch {}
    await delay(200);
  }
  throw new Error(`Local service did not start: ${url}`);
}
try {
  const backendArgs = ['--root', resolve(workspace), '--port', '8765', '--token-stdin'];
  if (model) backendArgs.push('--model-path', resolve(model));
  if (llama) backendArgs.push('--llama-server', resolve(llama));
  const backend = run(resolve(backendPath), backendArgs, { cwd: root });
  backend.stdin.end(token + '\n');
  await ready('http://127.0.0.1:8765/api/board', { 'X-Atlas-Token': token });
  run(resolve(ui, 'node_modules/.bin/vite'), [], { cwd: ui, env: { ...process.env, ATLAS_DEV_TOKEN: token } });
  await ready('http://127.0.0.1:5173/api/board');
  const app = run('cargo', ['run', '--offline'], { cwd: here });
  const code = await new Promise((resolveExit, reject) => { app.once('error', reject); app.once('exit', code => resolveExit(code ?? 1)); });
  process.exitCode = code;
} catch (error) {
  console.error(String(error));
  process.exitCode = 1;
} finally { cleanup(); }
