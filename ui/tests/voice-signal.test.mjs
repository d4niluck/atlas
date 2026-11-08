import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('индикатор отражает фактический сигнал, а не пульсирует на тишине',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-signal-test-'));
 try {const output=join(dir,'signal.mjs');await build({entryPoints:['src/voiceSignal.ts'],outfile:output,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {signalLevel}=await import(pathToFileURL(output).href);
  assert.equal(signalLevel(new Float32Array(128)),0);
  assert.ok(signalLevel(Float32Array.from([.15,-.15,.15,-.15]))>signalLevel(Float32Array.from([.01,-.01,.01,-.01])));
  assert.ok(signalLevel(Float32Array.from([1,-1]))<=1);
 } finally {await rm(dir,{recursive:true,force:true})}
});
