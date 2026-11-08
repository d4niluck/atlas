import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function withComponents(run){
 const dir=await mkdtemp(join(process.cwd(),'.atlas-quality-test-'));
 const saved=['window','navigator','__voiceHooks'].map(key=>[key,Object.getOwnPropertyDescriptor(globalThis,key)]);
 const plugin={name:'voice-react-fixture',setup(b){
  b.onResolve({filter:/^react$/},()=>({path:'react',namespace:'voice-fixture'}));
  b.onResolve({filter:/^react\/jsx-runtime$/},()=>({path:'jsx',namespace:'voice-fixture'}));
  b.onResolve({filter:/^\.\/voiceSession$/},()=>({path:'session',namespace:'voice-fixture'}));
  b.onResolve({filter:/^\.\/voiceWav$/},()=>({path:'wav',namespace:'voice-fixture'}));
  b.onLoad({filter:/.*/,namespace:'voice-fixture'},a=>{
   if(a.path==='react')return {contents:`export function useState(initial){const h=globalThis.__voiceHooks;const i=h.index++;const v=i in h.values?h.values[i]:initial;return [v,x=>h.updates.push([i,x])]}export function useRef(v){return {current:v}} export function useEffect(){} export default {};`,loader:'js'};
   if(a.path==='jsx')return {contents:`export function jsx(type,props){return {type,props:props||{}}}export const jsxs=jsx;export const Fragment='fragment';`,loader:'js'};
   if(a.path==='session')return {contents:`export function createVoiceSession(){return {start:async()=>{},stop:async()=>null,cancel:async()=>{}}}`,loader:'js'};
   return {contents:`export function encodeWav(){return new Uint8Array()}`,loader:'js'};
  });
 }};
 try{
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{platform:'MacIntel'}});
  const out=join(dir,'component.mjs');await build({entryPoints:['src/VoiceInput.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',plugins:[plugin],loader:{'.css':'empty'},logLevel:'silent'});
  await run(await import(pathToFileURL(out).href));
 }finally{
  for(const [key,desc] of saved){if(desc)Object.defineProperty(globalThis,key,desc);else delete globalThis[key]}
  await rm(dir,{recursive:true,force:true});
 }
}
function hooks(values=[]){globalThis.__voiceHooks={index:0,values,updates:[]};return globalThis.__voiceHooks}
function children(n){return n?.props?.children}
function find(node,fn){if(Array.isArray(node)){for(const child of node){const found=find(child,fn);if(found)return found}return}if(!node||typeof node!=='object')return;if(fn(node))return node;return find(children(node),fn)}
const flush=()=>new Promise(resolve=>setImmediate(resolve));

test('оба окна перед началом записи читают актуальный выбор и не загружают модель',async()=>withComponents(async({VoiceInput})=>{
 let current='small';const calls=[];
 Object.defineProperty(globalThis,'window',{configurable:true,value:{__TAURI__:{core:{invoke:async(name,args)=>{calls.push([name,args]);if(name==='voice_get_quality')return current;if(name==='voice_model_status')return 'missing';throw Error('unexpected call '+name)}}}}});
 async function clickFromWindow(){hooks();const tree=VoiceInput({onText(){}});const mic=find(tree,n=>n.type==='button'&&n.props['aria-label']==='Голосовой ввод');assert.ok(mic);mic.props.onClick();await flush()}
 await clickFromWindow();current='medium';await clickFromWindow();await clickFromWindow();
 assert.deepEqual(calls.filter(([name])=>name==='voice_model_status').map(([,args])=>args.quality),['small','medium','medium']);
 assert.ok(!calls.some(([name])=>name==='voice_download_model'));
}));

test('ошибка сохранения не меняет выбранную модель и не запускает загрузку',async()=>withComponents(async({VoiceSettings})=>{
 const calls=[];Object.defineProperty(globalThis,'window',{configurable:true,value:{__TAURI__:{core:{invoke:async(name,args)=>{calls.push([name,args]);if(name==='voice_set_quality')throw Error('disk failed');throw Error('unexpected call '+name)}}}}});
 const h=hooks(['small','installed','',false]);const tree=VoiceSettings();
 const buttons=[];function collect(node){if(Array.isArray(node))return node.forEach(collect);if(!node||typeof node!=='object')return;if(node.type==='button')buttons.push(node);collect(children(node))}collect(tree);
 const high=buttons.find(button=>JSON.stringify(children(button)).includes('Высокое'));assert.ok(high);high.props.onClick();await flush();
 assert.deepEqual(calls.map(([name])=>name),['voice_set_quality']);assert.ok(!h.updates.some(([index])=>index===0),'quality must stay unchanged');
 assert.ok(h.updates.some(([index,value])=>index===2&&String(value).includes('Не удалось сохранить')));
}));
