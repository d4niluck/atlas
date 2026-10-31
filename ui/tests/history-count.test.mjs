import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
test('counts displayed fragments, including title-only hits',async()=>{const d=await mkdtemp(join(tmpdir(),'atlas-history-'));try{const f=join(d,'history.mjs');await build({entryPoints:['src/SearchHistory.tsx'],outfile:f,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd()});const {countHistoryResults}=await import(pathToFileURL(f).href);assert.equal(countHistoryResults([{id:'a',title:'A',chunks:[{text:'x'},{text:'y'}]},{id:'b',title:'B',chunks:[]}]),3);assert.equal(countHistoryResults([]),0)}finally{await rm(d,{recursive:true,force:true})}});
test('history preview includes the answer, not just its label',async()=>{const d=await mkdtemp(join(tmpdir(),'atlas-history-'));try{const f=join(d,'history.mjs');await build({entryPoints:['src/SearchHistory.tsx'],outfile:f,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd()});const {historyPreview}=await import(pathToFileURL(f).href);assert.equal(historyPreview('rag','  Найден ответ [1]  '),'Ответ: Найден ответ [1]');assert.equal(historyPreview('rag',''),'Ответ: нет ответа');assert.equal(historyPreview('semantic'),'По смыслу')}finally{await rm(d,{recursive:true,force:true})}});
