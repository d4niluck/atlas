import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp, rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('Windows home hides unsupported workspace controls and leaves local operations', async () => {
 const dir=await mkdtemp(join(process.cwd(), '.atlas-win-ui-'));
 try {
  const out=join(dir,'home.mjs');
  await build({entryPoints:['src/WorkspaceHome.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',external:['react'],loader:{'.css':'empty'}});
  const {WorkspaceHome}=await import(pathToFileURL(out).href);
  const f=async()=>{};
  const props={moveRecoveryWarnings:[],recoveryWarnings:[],workspaces:[{id:'a',name:'Личное'},{id:'b',name:'Внешнее',external:true,available:true},{id:'c',name:'Секрет',protected:true}],opened:[],onProtection:f,onOpen:f,onCreate:f,onDelete:f,onRename:f,onDuplicate:f,onImport:f,onExport:f,onMove:f,onAttach:f,onCancelExport:f,retryExport:false};
  const windows=renderToStaticMarkup(React.createElement(WorkspaceHome,{...props,windowsMvp:true}));
  for(const label of ['Открыть пространство с носителя','Импортировать пространство','Сохранить копии всех пространств','Защитить паролем','Сохранить копию пространства']) assert.ok(!windows.includes(label),label);
  for(const label of ['Создать пространство','Переименовать пространство','Удалить пространство','Дублировать пространство']) assert.ok(windows.includes(label),label);
  for (const [name, notice] of [['Внешнее','Внешнее пространство недоступно в Windows'],['Секрет','Защищённое пространство недоступно в Windows']]) {
   assert.match(windows, new RegExp('<button class="portable-name" disabled="">'+name+'</button>'));
   assert.ok(windows.includes(notice),notice);
   for (const action of ['Дублировать','Переименовать',name==='Внешнее'?'Отсоединить':'Удалить']) {
    assert.match(windows, new RegExp('<button[^>]*disabled=""[^>]*aria-label="'+action+' пространство '+name+'"'));
   }
  }
  assert.match(windows, /<button class="portable-name">Личное<\/button>/);
  const mac=renderToStaticMarkup(React.createElement(WorkspaceHome,props));
  for(const label of ['Открыть пространство с носителя','Импортировать пространство','Сохранить копии всех пространств','Защитить паролем','Сохранить копию пространства']) assert.ok(mac.includes(label),label);
  assert.match(mac, /<button class="portable-name">Внешнее<\/button>/);
  assert.match(mac, /<button class="portable-name">Секрет<\/button>/);
 } finally { await rm(dir,{recursive:true,force:true}); }
});
