import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp, rm, readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('waiting indicator announces work and respects reduced motion', async () => {
  const dir = await mkdtemp(join(process.cwd(), '.atlas-wait-test-'));
  try {
    const output = join(dir, 'component.mjs');
    await build({entryPoints:['src/AnswerWaiting.tsx'], outfile:output, bundle:true, platform:'node', format:'esm', external:['react'], loader:{'.css':'empty'}});
    const {AnswerWaiting} = await import(pathToFileURL(output).href);
    const html = renderToStaticMarkup(React.createElement(AnswerWaiting));
    assert.match(html, /role="status"/);
    assert.match(html, /Ищем и готовим ответ/);
    assert.match(html, /aria-hidden="true"/);
    const css = await readFile('src/answer-waiting.css', 'utf8');
    assert.match(css, /prefers-reduced-motion:reduce/);
    assert.match(css, /data-theme=light/);
  } finally { await rm(dir, {recursive:true, force:true}); }
});
