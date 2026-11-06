import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

const css = await readFile(new URL('../src/stack-layers.css', import.meta.url), 'utf8');
const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selector, declarations]) => ({selector: selector.replace(/\/\*[\s\S]*?\*\//g, '').trim(), declarations}));

test('moving the board does not remove existing group blur', () => {
  const overrides = rules.filter(r => /viewport-moving/.test(r.selector) && /(?:-webkit-)?backdrop-filter\s*:\s*none/.test(r.declarations));
  assert.deepEqual(overrides, []);
});

test('expanded groups do not unconditionally blur background', () => {
  const unconditional = rules.filter(r => r.selector.split(',').some(s => /^\s*\.stack\.stack-(?:open|front)\s*$/.test(s)) && /(?:-webkit-)?backdrop-filter\s*:\s*blur\(/.test(r.declarations));
  assert.deepEqual(unconditional, []);
});
