import {test} from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function renderWithState(messages,error='',readiness=null) {
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-history-test-'));
 try {
  const plugin={name:'chat-state',setup(b){b.onLoad({filter:/ChatPanel\.tsx$/},async args=>{const {readFile}=await import('node:fs/promises');let source=await readFile(args.path,'utf8');source=source.replace("import React, {useEffect, useLayoutEffect, useRef, useState} from 'react';", "import React, {useEffect, useLayoutEffect, useRef, useState as actualUseState} from 'react';\nlet stateIndex=0; function useState(value){const i=stateIndex++; if(i===0)value=globalThis.__chatMessages; if(i===6)value=globalThis.__chatError; return actualUseState(value)}");return {contents:source,loader:'tsx'};});}};
  const out=join(dir,'chat.mjs');await build({entryPoints:['src/ChatPanel.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',external:['react','react-dom'],loader:{'.css':'empty'},plugins:[plugin],logLevel:'silent'});
  globalThis.__chatMessages=messages;globalThis.__chatError=error;
  const {ChatPanel}=await import(pathToFileURL(out).href);return renderToStaticMarkup(React.createElement(ChatPanel,{prefix:'/api/w/test',objectIds:[],visible:true,onOpen(){},readiness,onOpenSettings(){}}));
 } finally {delete globalThis.__chatMessages;delete globalThis.__chatError;await rm(dir,{recursive:true,force:true})}
}

test('история показывает свёрнутый журнал действий у каждого ответа',async()=>{
 const html=await renderWithState([{seq:1,role:'user',content:'Вопрос 1'},{seq:2,role:'assistant',content:'Ответ 1',steps:['Ищем документы','Готовим ответ']},{seq:3,role:'user',content:'Вопрос 2'},{seq:4,role:'assistant',content:'Ответ 2',steps:['Определяем план']}]);
 assert.equal((html.match(/<details class="chat-progress"/g)||[]).length,2);assert.doesNotMatch(html,/<details class="chat-progress" open/);
 for(const text of ['Ищем документы','Готовим ответ','Определяем план','Ответ 1','Ответ 2'])assert.ok(html.includes(text));
});

test('ошибка загрузки истории предлагает повторить без закрытия панели',async()=>{
 const html=await renderWithState([],'Не удалось загрузить историю чата');
 assert.match(html,/Не удалось загрузить историю чата/);assert.match(html,/>Повторить<\/button>/);
});


test('при выключенном умном поиске чат предлагает открыть настройки',async()=>{
 const html=await renderWithState([],'','Для чата включите умный поиск в настройках.');
 assert.match(html,/Для чата включите умный поиск/);assert.match(html,/>Открыть настройки поиска<\/button>/);
});
