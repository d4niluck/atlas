import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('stopping during indexing wait does not cancel a finished request; stopping generation does',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-menu-stop-'));
 try{
  const out=join(dir,'component.mjs');
  await build({entryPoints:['src/MenuBarSearch.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',external:['react','react-dom'],loader:{'.css':'empty'}});
  const {clearFinishedAttempt,cancelRagAttempt}=await import(pathToFileURL(out).href);
  const calls=[];const fetcher=async url=>{calls.push(url);return new Response('{}')};
  const operation={id:'finished',prefix:'/api/w/a',poll:new AbortController()};
  clearFinishedAttempt(operation);
  await cancelRagAttempt(operation,fetcher);
  assert.deepEqual(calls,[]);
  assert.equal(operation.poll.signal.aborted,true);
  operation.id='active';operation.poll=new AbortController();
  await cancelRagAttempt(operation,fetcher);
  assert.deepEqual(calls,['/api/w/a/rag/cancel/active']);
 }finally{await rm(dir,{recursive:true,force:true})}
});

test('late cancel failure does not overwrite a newer search',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-menu-stop-'));
 try{
  const out=join(dir,'component.mjs');
  await build({entryPoints:['src/MenuBarSearch.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',external:['react','react-dom'],loader:{'.css':'empty'}});
  const {reportCancelFailure}=await import(pathToFileURL(out).href);
  let resolve;const fetcher=()=>new Promise(r=>{resolve=r});let generation=1;const errors=[];
  const pending=reportCancelFailure({id:'old',prefix:'/api/w/a'},()=>generation===1,()=>errors.push('cancel failed'),fetcher);
  generation=2;resolve(new Response('',{status:400}));await pending;
  assert.deepEqual(errors,[]);
  await reportCancelFailure({id:'current',prefix:'/api/w/a'},()=>generation===2,()=>errors.push('cancel failed'),async()=>new Response('',{status:400}));
  assert.deepEqual(errors,['cancel failed']);
 }finally{await rm(dir,{recursive:true,force:true})}
});
