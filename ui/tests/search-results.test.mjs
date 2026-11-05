import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { renderToStaticMarkup } from 'react-dom/server';
import React from 'react';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

test('results show ranked fragments as clickable entries', async () => {
  const dir = await mkdtemp(join(process.cwd(), '.atlas-search-test-'));
  try {
    const output = join(dir, 'component.mjs');
    await build({entryPoints:['src/SearchResults.tsx'], outfile:output, bundle:true, platform:'node', format:'esm', absWorkingDir:process.cwd(), external:['react'], loader:{'.css':'empty'}});
    const {SearchResults} = await import(pathToFileURL(output).href);
    const html = renderToStaticMarkup(React.createElement(SearchResults, {results:[{id:"a",title:"Источник",chunks:[{text:"Найденный фрагмент",start:0,end:18}]}],onOpen:()=>{}}));
    assert.doesNotMatch(html, /<details/);
    assert.doesNotMatch(html, /<summary/);
    assert.match(html, /Найденный фрагмент/);
    assert.match(html, /Источник/);
    const longText = '😀'.repeat(241);
    const longHtml = renderToStaticMarkup(React.createElement(SearchResults, {results:[{id:'a',title:'Длинный',chunks:[{text:longText,start:0,end:241}]}],onOpen:()=>{}}));
    assert.match(longHtml, /Показать полностью/);
    assert.match(longHtml, /aria-expanded="false"/);
    assert.ok(longHtml.includes('😀'.repeat(240) + '…'));
    assert.ok(!longHtml.includes(longText));
    const stale = renderToStaticMarkup(React.createElement(SearchResults, {results:[{id:"a",title:"Источник",chunks:[{text:"старый текст",start:0,end:12,stale:true}]}],onOpen:()=>{}}));
    assert.match(stale, /Текст изменился/);
    const missing = renderToStaticMarkup(React.createElement(SearchResults, {results:[{id:"a",title:"Источник",missing:true,chunks:[{text:"текст",start:0,end:5,stale:true}]}],onOpen:()=>{}}));
    assert.match(missing, /Источник удалён/);
    assert.equal((missing.match(/disabled=""/g)||[]).length, 1);
  } finally { await rm(dir, {recursive:true, force:true}); }
});
