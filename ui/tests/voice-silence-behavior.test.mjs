import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function withDetector(run){
 const dir=await mkdtemp(join(process.cwd(),'.atlas-silence-test-'));
 try{
  const out=join(dir,'signal.mjs');
  await build({entryPoints:['src/voiceSignal.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});
  const module=await import(pathToFileURL(out).href);
  assert.equal(typeof module.createSilenceDetector,'function','ожидается контроль тишины');
  await run(module.createSilenceDetector);
 }finally{await rm(dir,{recursive:true,force:true})}
}
const rate=16000;
function audio(level,ms){const result=new Float32Array(Math.round(rate*ms/1000));for(let i=0;i<result.length;i++)result[i]=(i%2?1:-1)*level;return result}

test('долгое ожидание речи и тихий шум не завершают запись',async()=>withDetector(create=>{
 const d=create(rate);
 for(let i=0;i<80;i++)assert.equal(d.feed(audio(0.0005,100)),false);
}));

test('после речи запись завершается ровно через две секунды непрерывной тишины один раз',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.05,200)),false);
 assert.equal(d.feed(audio(0,1900)),false);
 assert.equal(d.feed(audio(0,99)),false);
 assert.equal(d.feed(audio(0,1)),true);
 assert.equal(d.feed(audio(0,1000)),false);
}));

test('продолжение речи сбрасывает отсчёт тишины, в том числе более тихая речь',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.05,200)),false);
 assert.equal(d.feed(audio(0,1500)),false);
 assert.equal(d.feed(audio(0.003,300)),false);
 assert.equal(d.feed(audio(0,1900)),false);
 assert.equal(d.feed(audio(0,100)),true);
}));

test('короткий щелчок не принимается за начало речи',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.1,50)),false);
 assert.equal(d.feed(audio(0,3000)),false);
 assert.equal(d.feed(audio(0.05,200)),false);
 assert.equal(d.feed(audio(0,2000)),true);
}));

test('длительность вычисляется по частоте дискретизации, а не числу вызовов',async()=>withDetector(create=>{
 for(const sampleRate of [8000,44100,48000]){
  const d=create(sampleRate);const block=(value,ms)=>Float32Array.from({length:Math.round(sampleRate*ms/1000)},(_,i)=>(i%2?1:-1)*value);
  assert.equal(d.feed(block(0.05,200)),false);
  assert.equal(d.feed(block(0,1900)),false);
  assert.equal(d.feed(block(0,100)),true);
 }
}));

test('короткий щелчок внутри длинного блока не включает автоостановку',async()=>withDetector(create=>{
 const d=create(rate);const block=audio(0,256);for(let i=0;i<800;i++)block[i]=(i%2?1:-1)*0.1;
 assert.equal(d.feed(block),false);
 assert.equal(d.feed(audio(0,2000)),false,'короткий шум не должен включить отсчёт до речи');
}));

test('короткая фраза из 160 мс не теряется, если после неё 2 секунды тишины',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.02,160)),false);
 assert.equal(d.feed(audio(0,2000)),true);
}));

test('постепенно затихающая речь не обрывается до двух секунд тишины',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.05,200)),false);
 for(const level of [0.02,0.01,0.006,0.003])assert.equal(d.feed(audio(level,400)),false);
 assert.equal(d.feed(audio(0.001,1900)),false);
 assert.equal(d.feed(audio(0.001,100)),true);
}));

test('тихий фоновый шум не прерывает отсчёт после речи и не считается речью до начала',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.001,4000)),false);
 assert.equal(d.feed(audio(0.02,200)),false);
 assert.equal(d.feed(audio(0.001,1900)),false);
 assert.equal(d.feed(audio(0.001,100)),true);
}));

test('короткая естественная пауза между слогами не мешает определить начало речи',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.02,100)),false);
 assert.equal(d.feed(audio(0,40)),false);
 assert.equal(d.feed(audio(0.02,100)),false);
 assert.equal(d.feed(audio(0,2000)),true);
}));

test('разделённые паузой щелчки не составляют речь',async()=>withDetector(create=>{
 const d=create(rate);assert.equal(d.feed(audio(0.1,100)),false);
 assert.equal(d.feed(audio(0,120)),false);
 assert.equal(d.feed(audio(0.1,100)),false);
 assert.equal(d.feed(audio(0,2000)),false);
}));
