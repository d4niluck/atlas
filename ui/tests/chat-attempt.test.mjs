import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('при смене фильтра старый поток перестаёт отображаться до подтверждения остановки',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-attempt-test-'));
 try{const out=join(dir,'attempt.mjs');await build({entryPoints:['src/chatAttempt.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {cancelForScopeChange,acceptsChatEvents}=await import(pathToFileURL(out).href);
  const attempt={id:'old',request:new AbortController(),poll:new AbortController(),scopeChanged:false};let resolve;const server=new Promise(r=>resolve=r);
  assert.equal(acceptsChatEvents(attempt,attempt),true);const cancellation=cancelForScopeChange(attempt,()=>server);
  assert.equal(attempt.scopeChanged,true);assert.equal(attempt.request.signal.aborted,true);assert.equal(attempt.poll.signal.aborted,true);assert.equal(acceptsChatEvents(attempt,attempt),false);
  resolve();await cancellation;
 }finally{await rm(dir,{recursive:true,force:true})}
});
