import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('compact results keep fragment expansion and source navigation separate', async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-menu-results-test-'));
 try {
  const out=join(dir,'component.mjs');
  await build({entryPoints:['src/MenuSearchResults.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd(),external:['react'],loader:{'.css':'empty'}});
  const {MenuSearchResults}=await import(pathToFileURL(out).href);
  const text='Фрагмент '.repeat(45);
  const html=renderToStaticMarkup(React.createElement(MenuSearchResults,{results:[{id:'source-1',title:'Документ',chunks:[{text,start:23,end:31}]}],onOpen:()=>{}}));
  assert.match(html,/<details\b/, 'фрагмент можно раскрыть');
  assert.match(html,/<summary\b/, 'краткий фрагмент является элементом раскрытия');
  assert.match(html,/Документ/);
  assert.match(html,/Открыть источник/, 'переход к документу расположен на отдельной кнопке');
  assert.match(html,/<button\b[^>]*>/, 'переход к источнику доступен с клавиатуры');
  assert.ok(html.includes(text), 'полный текст доступен после раскрытия');
  const summary=html.match(/<summary\b[^>]*>([\s\S]*?)<\/summary>/)?.[1];
  assert.ok(summary && !summary.includes(text), 'в свёрнутом виде показывается только часть длинного фрагмента');
  assert.ok(!summary.includes('<button'), 'кнопка перехода не вложена в элемент раскрытия');
 } finally {await rm(dir,{recursive:true,force:true})}
});

test('empty results and deleted sources are handled clearly', async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-menu-results-test-'));
 try {
  const out=join(dir,'component.mjs');
  await build({entryPoints:['src/MenuSearchResults.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd(),external:['react'],loader:{'.css':'empty'}});
  const {MenuSearchResults}=await import(pathToFileURL(out).href);
  const empty=renderToStaticMarkup(React.createElement(MenuSearchResults,{results:[],onOpen:()=>{}}));
  assert.match(empty,/Ничего не найдено/);
  const missing=renderToStaticMarkup(React.createElement(MenuSearchResults,{results:[{id:'gone',title:'Документ',missing:true,chunks:[{text:'Фрагмент',start:0,end:8}]}],onOpen:()=>{}}));
  assert.match(missing,/Источник удалён/);
  assert.match(missing,/<button\b[^>]*disabled=""/);
 } finally {await rm(dir,{recursive:true,force:true})}
});
