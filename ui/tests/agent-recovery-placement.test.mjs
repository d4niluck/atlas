import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {strict as assert} from 'node:assert';

const home = readFileSync(new URL('../src/WorkspaceHome.tsx', import.meta.url), 'utf8');
const settings = readFileSync(new URL('../src/AgentSettings.tsx', import.meta.url), 'utf8');

test('agent recovery is available in history, not on the home screen', () => {
  assert.doesNotMatch(home, /<AgentCreateRecovery\b/);
  assert.match(settings, /showEvents\s*&&[\s\S]*<AgentCreateRecovery\s+workspaces=\{spaces\}/);
});
