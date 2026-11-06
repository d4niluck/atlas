import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function component(name, fn) {
 const dir=await mkdtemp(join(process.cwd(),'.atlas-visual-test-'));
 try {
  const out=join(dir,'component.mjs');
  await build({entryPoints:[`src/${name}.tsx`],outfile:out,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd(),external:['react','react-dom'],loader:{'.css':'empty'}});
  await fn((await import(pathToFileURL(out).href))[name]);
 } finally {await rm(dir,{recursive:true,force:true})}
}

test('short fragments show their text without an unnecessary expand control',async()=>{
 await component('MenuSearchResults',MenuSearchResults=>{
  for(const text of ['Короткий текст','Я'.repeat(180),'🌟'.repeat(180),'Короткий текст'+' '.repeat(200)]) {
   const html=renderToStaticMarkup(React.createElement(MenuSearchResults,{results:[{id:'one',title:'Документ',chunks:[{text,start:0,end:1}]}],onOpen:()=>{}}));
   assert.ok(html.includes(text.trim()),'фрагмент виден полностью');
   assert.doesNotMatch(html,/<details\b|<summary\b|Развернуть|Свернуть/,'для короткого фрагмента нет раскрытия');
   assert.match(html,/Открыть источник/,'кнопка источника сохранена');
  }
 });
});

test('long fragments retain expansion and a separate source control',async()=>{
 await component('MenuSearchResults',MenuSearchResults=>{
  const text='Я'.repeat(181);
  const html=renderToStaticMarkup(React.createElement(MenuSearchResults,{results:[{id:'one',title:'Документ',chunks:[{text,start:0,end:1}]}],onOpen:()=>{}}));
  assert.match(html,/<details\b/);
  assert.match(html,/<summary\b/);
  assert.ok(html.includes(text),'полный текст доступен при раскрытии');
  assert.match(html,/Открыть источник/);
 });
});

test('compact menu keeps settings but does not offer app exit or open buttons',async()=>{
 await component('MenuBarSearch',MenuBarSearch=>{
  const html=renderToStaticMarkup(React.createElement(MenuBarSearch,{onNavigate:()=>{},onOpenAtlas:()=>{},onExit:()=>{},onOpenSettings:()=>{}}));
  assert.match(html,/Настройки поиска/);
  assert.doesNotMatch(html,/Открыть Atlas|Выйти из Atlas/);
 });
});
