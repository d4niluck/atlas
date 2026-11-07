import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

const layers = await readFile(new URL('../src/stack-layers.css', import.meta.url), 'utf8');
const main = await readFile(new URL('../src/main.tsx', import.meta.url), 'utf8');
const rules = [...layers.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, selector, body]) => ({selector: selector.replace(/\/\*[\s\S]*?\*\//g, '').trim(), body}));
function declarations(selector) {
 return rules.filter(r => r.selector.split(',').some(s => s.trim() === selector)).map(r => r.body).join(';');
}
function z(selector) { return Number(declarations(selector).match(/(?:^|;)\s*z-index\s*:\s*(\d+)/)?.[1]); }
function background(selector) { return declarations(selector).match(/(?:^|;)\s*background(?:-color)?\s*:\s*([^;]+)/)?.[1]?.trim(); }
function assertOpaque(color) {
 assert.ok(color, 'an explicit background colour is required');
 assert.match(color, /^#[\da-f]{3}(?:[\da-f]{3})?$/i, `expected an opaque colour, got ${color}`);
}

test('expanded groups cover objects behind them in both themes', () => {
 for (const selector of ['.stack.stack-open','.stack.stack-front',':root[data-theme=light] .stack.stack-open',':root[data-theme=light] .stack.stack-front']) assertOpaque(background(selector));
});

test('expanded groups do not blur the board or calculate overlap solely to blur', () => {
 assert.doesNotMatch(layers, /(?:-webkit-)?backdrop-filter\s*:\s*blur\s*\(/);
 assert.doesNotMatch(main, /\bstackNeedsBlur\b/);
});

test('group cards and internal connections remain above their backgrounds', () => {
 assert.ok(z('.card.group-card-open') > z('.stack.stack-open'));
 assert.ok(z('.lines.lines-group') > z('.stack.stack-open'));
 assert.ok(z('.card.group-card-front') > z('.stack.stack-front'));
 assert.ok(z('.lines.lines-front') >= z('.stack.stack-front'));
});

test('internal connections remain visible with matching arrows against the light group surface', () => {
 const bg = background(':root[data-theme=light] .stack.stack-open');
 const luminance = color => {
  assert.match(color, /^#[\da-f]{6}$/i);
  const channels = [1,3,5].map(i => parseInt(color.slice(i,i+2),16)/255).map(v => v <= .04045 ? v/12.92 : ((v+.055)/1.055)**2.4);
  return channels[0]*.2126 + channels[1]*.7152 + channels[2]*.0722;
 };
 for (const group of ['.lines-group','.lines-front']) {
  const stroke = declarations(`:root[data-theme=light] ${group} .edge-path`).match(/(?:^|;)\s*stroke\s*:\s*([^;]+)/)?.[1]?.trim();
  const fill = declarations(`:root[data-theme=light] ${group} marker path`).match(/(?:^|;)\s*fill\s*:\s*([^;]+)/)?.[1]?.trim();
  assert.ok(stroke, `${group} requires a visible light-theme stroke`);
  assert.equal(fill, stroke, `${group} marker should match its line`);
  const [a,b]=[luminance(bg),luminance(stroke)].sort((x,y)=>y-x);
  assert.ok((a+.05)/(b+.05) >= 3, `${group} needs at least 3:1 contrast`);
 }
});

test('dark theme keeps the existing connection colour', async () => {
 const baseStyles = await readFile(new URL('../src/style.css', import.meta.url), 'utf8');
 assert.match(baseStyles, /\.edge-path\s*\{[^}]*stroke\s*:\s*#9385b6\b/i);
 for (const marker of ['edge-arrow-group','edge-arrow-front']) {
  assert.match(main, new RegExp(`<marker id="${marker}"[^>]*><path[^>]*fill="#9385b6"`, 'i'));
 }
 assert.ok(!rules.some(r => /(?:^|,)\s*(?!:root\[data-theme=light\])[^,]*\.lines-(?:group|front)[^,]*(?:\.edge-path|marker\s+path)/.test(r.selector) && /\b(?:stroke|fill)\s*:/.test(r.body)), 'no dark-theme colour override');
});
