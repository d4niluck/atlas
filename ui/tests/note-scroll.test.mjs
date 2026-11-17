import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
test('scroll synchronizes both directions without a feedback loop',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-scroll-'));
 try{const out=join(dir,'scroll.mjs');await build({entryPoints:['src/note-scroll.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {syncScroll}=await import(pathToFileURL(out).href);
 const a={scrollTop:200,scrollHeight:500,clientHeight:100},b={scrollTop:0,scrollHeight:900,clientHeight:100},pending=new WeakMap();syncScroll(a,b,pending);assert.equal(b.scrollTop,400);syncScroll(b,a,pending);assert.equal(a.scrollTop,200);b.scrollTop=800;syncScroll(b,a,pending);assert.equal(a.scrollTop,400);a.scrollHeight=100;syncScroll(a,b,new WeakMap());assert.equal(b.scrollTop,0);
 }finally{await rm(dir,{recursive:true,force:true})}
});
