import {test} from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function render(entry,name,props={}){
 const dir=await mkdtemp(join(process.cwd(),'.atlas-voice-ui-test-'));
 const oldNavigator=Object.getOwnPropertyDescriptor(globalThis,'navigator');
 const oldLocalStorage=Object.getOwnPropertyDescriptor(globalThis,'localStorage');
 try{
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{platform:'MacIntel'}});
  Object.defineProperty(globalThis,'localStorage',{configurable:true,value:{getItem:()=>null}});
  const out=join(dir,'component.mjs');
  await build({entryPoints:[entry],outfile:out,bundle:true,platform:'node',format:'esm',external:['react','react-dom'],loader:{'.css':'empty'},logLevel:'silent'});
  const component=(await import(pathToFileURL(out).href))[name];
  return renderToStaticMarkup(React.createElement(component,props));
 }finally{
  if(oldNavigator)Object.defineProperty(globalThis,'navigator',oldNavigator);else delete globalThis.navigator;
  if(oldLocalStorage)Object.defineProperty(globalThis,'localStorage',oldLocalStorage);else delete globalThis.localStorage;
  await rm(dir,{recursive:true,force:true});
 }
}

test('в строке меню рядом с запросом доступен голосовой ввод',async()=>{
 const html=await render('src/MenuBarSearch.tsx','MenuBarSearch',{onNavigate(){},onOpenSettings(){}});
 assert.match(html,/aria-label="Голосовой ввод"/);
 assert.ok(html.indexOf('aria-label="Поисковый запрос"')<html.indexOf('aria-label="Голосовой ввод"'));
});

test('настройки показывают три уровня качества и размеры моделей',async()=>{
 const html=await render('src/VoiceInput.tsx','VoiceSettings');
 for(const text of ['Низкое','Среднее','Высокое','60 МБ','190 МБ','539 МБ'])assert.ok(html.includes(text),`не видно «${text}»`);
});
