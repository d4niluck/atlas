import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('чат объясняет необходимые настройки и не требует преждевременной загрузки модели в память',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-readiness-test-'));try{const out=join(dir,'readiness.mjs');await build({entryPoints:['src/chatReadiness.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {chatReadiness}=await import(pathToFileURL(out).href);
 const ready={available:true,message:'',model:'e2b',installed_e2b:true};
 assert.match(chatReadiness({semantic_enabled:false,rag_enabled:false},ready,''),/смысловой и умный поиск/);
 assert.match(chatReadiness({semantic_enabled:true,rag_enabled:false},ready,''),/включите умный поиск/);
 assert.match(chatReadiness({semantic_enabled:true,rag_enabled:true},{...ready,available:false,installed_e2b:false},''),/Загрузите выбранную модель/);
 assert.match(chatReadiness({semantic_enabled:true,rag_enabled:true},ready,'Модель не загружена'),/модель смыслового поиска/);
 assert.equal(chatReadiness({semantic_enabled:true,rag_enabled:true},ready,''),null);
 }finally{await rm(dir,{recursive:true,force:true})}
});
