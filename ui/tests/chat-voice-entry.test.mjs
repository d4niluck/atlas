import {test} from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('в поле сообщения чата есть доступная кнопка голосового ввода',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-voice-test-'));
 const old=Object.getOwnPropertyDescriptor(globalThis,'navigator');
 try{
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{platform:'MacIntel'}});
  const out=join(dir,'chat.mjs');await build({entryPoints:['src/ChatPanel.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',external:['react','react-dom'],loader:{'.css':'empty'},logLevel:'silent'});
  const {ChatPanel}=await import(pathToFileURL(out).href);
  const html=renderToStaticMarkup(React.createElement(ChatPanel,{prefix:'/api/w/test',objectIds:[],visible:true,onOpen(){},readiness:null,onOpenSettings(){}}));
  const form=html.match(/<form\b[^>]*class="chat-compose"[\s\S]*?<\/form>/)?.[0];
  assert.ok(form,'форма сообщения должна существовать');
  assert.match(form,/aria-label="Сообщение"/);
  assert.match(form,/aria-label="Голосовой ввод"/,'голосовой ввод доступен в форме сообщения');
 }finally{if(old)Object.defineProperty(globalThis,'navigator',old);else delete globalThis.navigator;await rm(dir,{recursive:true,force:true})}
});
