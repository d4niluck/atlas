import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('очистка синхронизирует оба окна ровно один раз и не затрагивает другое пространство',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-sync-'));
 try{const out=join(dir,'sync.mjs');await build({entryPoints:['src/chatClearSync.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {subscribeChatClear,publishChatClear}=await import(pathToFileURL(out).href);
  class Window extends EventTarget{constructor(){super();this.__TAURI__={event:{listen:async(_,f)=>{this.listener=f;return()=>{this.listener=null}},emit:async(_,payload)=>{for(const w of windows)w.listener?.({payload})}}};this.localStorage={setItem:(key,newValue)=>{for(const w of windows)if(w!==this){const e=new Event('storage');Object.assign(e,{key,newValue});w.dispatchEvent(e)}}}}}
  const windows=[new Window(),new Window()], seen=[0,0,0];
  const cleanups=[subscribeChatClear(windows[0],'/w/a',()=>seen[0]++),subscribeChatClear(windows[1],'/w/a',()=>seen[1]++),subscribeChatClear(windows[1],'/w/b',()=>seen[2]++)];await new Promise(r=>setImmediate(r));
  await publishChatClear(windows[0],'/w/a');assert.deepEqual(seen,[1,1,0]);for(const stop of cleanups)stop();
 }finally{await rm(dir,{recursive:true,force:true})}
});

test('нативные события синхронизируют окна, когда storage недоступен',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-sync-native-'));
 try{const out=join(dir,'sync.mjs');await build({entryPoints:['src/chatClearSync.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {subscribeChatClear,publishChatClear}=await import(pathToFileURL(out).href);
  const listeners=[];class Window extends EventTarget{constructor(){super();this.localStorage={setItem(){throw Error('blocked')}};this.__TAURI__={event:{listen:async(_,f)=>{listeners.push(f);return()=>listeners.splice(listeners.indexOf(f),1)},emit:async(_,payload)=>listeners.slice().forEach(f=>f({payload}))}}}}
  const a=new Window(),b=new Window();let received=0;const off=subscribeChatClear(b,'/w/a',()=>received++);await new Promise(r=>setImmediate(r));await publishChatClear(a,'/w/a');assert.equal(received,1);off();
 }finally{await rm(dir,{recursive:true,force:true})}
});
