import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function withPage(run){const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-page-test-'));try{const out=join(dir,'page.mjs');await build({entryPoints:['src/chatHistoryPage.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});await run(await import(pathToFileURL(out).href))}finally{await rm(dir,{recursive:true,force:true})}}
const message=(seq,content='')=>({seq,role:'user',content});
test('страницы истории идут по порядку без дубликатов и сохраняют исходные объекты',()=>withPage(({parseChatHistoryPage,mergeChatHistory})=>{
 const original=message(42,'новое');const first=parseChatHistoryPage({messages:[original,message(43)],next_before:42});const older=parseChatHistoryPage({messages:[message(40),message(41),message(42,'дубликат')],next_before:40});const merged=mergeChatHistory(first.messages,older.messages);assert.deepEqual(merged.map(m=>m.seq),[40,41,42,43]);assert.strictEqual(merged[2],original);
}));
test('некорректные страницы не заменяют сохранённую историю',()=>withPage(({parseChatHistoryPage})=>{
 for(const value of [{messages:[{role:'user',content:'x'}],next_before:null},{messages:[],next_before:0},{messages:[message(1)]},{messages:[{seq:1,role:'system',content:'x'}],next_before:null},{messages:[{...message(1),scope:9}],next_before:null},{messages:[{...message(1),steps:[{}]}],next_before:null},{messages:[{...message(1),found_sources:[{id:'x'}]}],next_before:null}])assert.throws(()=>parseChatHistoryPage(value),/История чата недоступна/);
}));
