import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('доставляет результаты и токены по порядку, игнорируя временный 404', async () => {
 const dir=await mkdtemp(join(tmpdir(),'atlas-rag-events-'));
 try {
  const out=join(dir,'events.mjs'); await build({entryPoints:['src/ragEvents.ts'],outfile:out,bundle:true,platform:'node',format:'esm'});
  const {watchRagEvents}=await import(pathToFileURL(out).href);
  const seen=[];let calls=0;const urls=[];
  await watchRagEvents('/api/w/example','req-1',new AbortController().signal,e=>seen.push(e),async url=>{urls.push(url);calls++;if(calls===1)return new Response('',{status:404});return new Response(JSON.stringify(calls===2?{events:[{seq:1,type:'results',results:[{id:'a',title:'A'}]},{seq:2,type:'token',text:'Привет'}],done:false}:{events:[{seq:3,type:'token',text:' мир'},{seq:4,type:'done'}],done:true}),{status:200})},0);
  assert.deepEqual(seen.map(x=>x.type),['results','token','token','done']);
  assert.match(urls[2],/after=2$/);
 } finally {await rm(dir,{recursive:true,force:true})}
});

test('не доставляет события после остановки', async () => {
 const dir=await mkdtemp(join(tmpdir(),'atlas-rag-events-'));
 try {
  const out=join(dir,'events.mjs'); await build({entryPoints:['src/ragEvents.ts'],outfile:out,bundle:true,platform:'node',format:'esm'});
  const {watchRagEvents}=await import(pathToFileURL(out).href);
  const controller=new AbortController(), seen=[];
  await watchRagEvents('/api/w/example','req-2',controller.signal,event=>{seen.push(event);controller.abort()},async()=>new Response(JSON.stringify({events:[{seq:1,type:'token',text:'a'},{seq:2,type:'token',text:'b'}],done:false}),{status:200}),0);
  assert.deepEqual(seen.map(e=>e.text),['a']);
 } finally {await rm(dir,{recursive:true,force:true})}
});

test('повтор после незавершённого индекса получает новый идентификатор', async () => {
 const dir=await mkdtemp(join(tmpdir(),'atlas-rag-retry-'));const original=globalThis.fetch;
 try {
  const out=join(dir,'retry.mjs');await build({entryPoints:['src/searchRetry.ts'],outfile:out,bundle:true,platform:'node',format:'esm'});
  const {searchWithRetry}=await import(pathToFileURL(out).href);const ids=[];let n=0;
  const options={method:'POST',body:JSON.stringify({request_id:'first'})};
  globalThis.fetch=async(_url,opts)=>{ids.push(JSON.parse(opts.body).request_id);return n++===0?new Response(JSON.stringify({error:'Подготавливаем индекс поиска. Попробуйте ещё раз через несколько секунд.'}),{status:503}):new Response('{}',{status:200})};
  let waiting=0;const result=await searchWithRetry('/rag/search',options,()=>waiting++,()=>{options.body=JSON.stringify({request_id:'second'})});
  assert.equal(result.status,200);assert.equal(waiting,1);assert.deepEqual(ids,['first','second']);
 } finally {globalThis.fetch=original;await rm(dir,{recursive:true,force:true})}
});
