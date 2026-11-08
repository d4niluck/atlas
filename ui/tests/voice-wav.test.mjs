import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function withEncoder(run){
 const dir=await mkdtemp(join(process.cwd(),'.atlas-wav-test-'));
 try{
  const out=join(dir,'encoder.mjs');
  await build({entryPoints:['src/voiceWav.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});
  const {encodeWav}=await import(pathToFileURL(out).href);
  await run(encodeWav);
 }finally{await rm(dir,{recursive:true,force:true})}
}
function view(bytes){return new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength)}
function text(bytes,start,end){return new TextDecoder().decode(bytes.slice(start,end))}

test('распознавателю передаётся моно WAV PCM16 с корректными размерами',async()=>withEncoder(async encode=>{
 const bytes=encode(new Float32Array([-2,-1,0,1,2]),16000),v=view(bytes);
 assert.equal(text(bytes,0,4),'RIFF');assert.equal(text(bytes,8,12),'WAVE');
 assert.equal(text(bytes,12,16),'fmt ');assert.equal(text(bytes,36,40),'data');
 assert.equal(v.getUint32(4,true),bytes.length-8);assert.equal(v.getUint32(40,true),bytes.length-44);
 assert.equal(v.getUint16(20,true),1);assert.equal(v.getUint16(22,true),1);
 assert.equal(v.getUint32(24,true),16000);assert.equal(v.getUint32(28,true),32000);
 assert.equal(v.getUint16(32,true),2);assert.equal(v.getUint16(34,true),16);
 assert.ok(v.getInt16(44,true)<=-32767);assert.ok(v.getInt16(46,true)<=-32767);
 assert.equal(v.getInt16(48,true),0);assert.ok(v.getInt16(50,true)>=32766);assert.ok(v.getInt16(52,true)>=32766);
}));

test('пересчёт частоты не меняет длительность записи',async()=>withEncoder(async encode=>{
 for(const sampleRate of [44100,48000]){
  const samples=new Float32Array(sampleRate).fill(0.25),bytes=encode(samples,sampleRate),v=view(bytes);
  assert.equal(v.getUint32(24,true),16000);
  assert.ok(Math.abs((bytes.length-44)/2-16000)<=1);
  assert.ok(Math.abs(v.getInt16(44,true)-8192)<=2);
 }
}));
