import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('после очистки переписка и возможность загрузить старые страницы исчезают',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-clear-test-'));const originalFetch=globalThis.fetch,originalWindow=globalThis.window;
 try{
  const plugin={name:'react-fixture',setup(b){
   b.onResolve({filter:/^react$/},()=>({path:'react',namespace:'fixture'}));
   b.onResolve({filter:/^react\/jsx-runtime$/},()=>({path:'jsx',namespace:'fixture'}));
   b.onResolve({filter:/^react-markdown$|^remark-|^rehype-/},a=>({path:a.path,namespace:'fixture'}));
   b.onLoad({filter:/.*/,namespace:'fixture'},a=>{
    if(a.path==='react')return {contents:`export function useState(v){let h=globalThis.__clearHooks,i=h.cursor++;if(!(i in h.values))h.values[i]=v;return [h.values[i],x=>h.values[i]=typeof x==='function'?x(h.values[i]):x]} export function useRef(v){let h=globalThis.__clearHooks,i=h.refCursor++;return h.refs[i]??=({current:v})}export function useEffect(){}export function useLayoutEffect(){}export function isValidElement(){return false} export default {memo:x=>x};`,loader:'js'};
    if(a.path==='jsx')return {contents:`export const Fragment=Symbol.for('fixture.fragment');export function jsx(type,props){return {type,props:props||{}}}export const jsxs=jsx;`,loader:'js'};
    return {contents:'export default function Stub(){}',loader:'js'};
   });
  }};
  const out=join(dir,'panel.mjs');await build({entryPoints:['src/ChatPanel.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',plugins:[plugin],loader:{'.css':'empty'},logLevel:'silent'});
  const {ChatPanel}=await import(pathToFileURL(out).href);globalThis.__clearHooks={cursor:0,refCursor:0,values:[],refs:[]};
  globalThis.__clearHooks.values[0]=[{seq:1,role:'user',content:'старый вопрос'}];globalThis.__clearHooks.values[8]=true;globalThis.__clearHooks.values[9]=1;
  globalThis.window=Object.assign(new EventTarget(),{localStorage:{setItem(){}}});
  let requests=[],allowClear=false;globalThis.fetch=async(url,options)=>{requests.push({url,method:options?.method});return {ok:allowClear,json:async()=>({messages:[],next_before:null})}};
  const render=()=>{globalThis.__clearHooks.cursor=0;globalThis.__clearHooks.refCursor=0;return ChatPanel({prefix:'/api/w/test',objectIds:[],visible:true,onOpen(){},readiness:null,onOpenSettings(){}})};
  const nodes=(root,pred)=>{const found=[];function visit(n){if(Array.isArray(n))return n.forEach(visit);if(!n||typeof n!=='object')return;if(pred(n))found.push(n);visit(n.props?.children)}visit(root);return found};
  let tree=render();nodes(tree,n=>n.type==='button'&&n.props['aria-label']==='Очистить контекст')[0].props.onClick();tree=render();
  let dialog=nodes(tree,n=>n.props?.role==='alertdialog')[0];assert.ok(dialog);nodes(dialog,n=>n.type==='button'&&n.props.children==='Очистить')[0].props.onClick();await new Promise(r=>setImmediate(r));tree=render();assert.equal(globalThis.__clearHooks.values[0][0].content,'старый вопрос','ошибка удаления не очищает историю');assert.equal(globalThis.__clearHooks.values[9],1);
  allowClear=true;dialog=nodes(tree,n=>n.props?.role==='alertdialog')[0];nodes(dialog,n=>n.type==='button'&&n.props.children==='Очистить')[0].props.onClick();await new Promise(r=>setImmediate(r));tree=render();
  assert.deepEqual(globalThis.__clearHooks.values[0],[]);assert.equal(globalThis.__clearHooks.values[9],null);assert.equal(globalThis.__clearHooks.values[6],'');assert.equal(requests[0].method,'POST');
 }finally{globalThis.fetch=originalFetch;if(originalWindow===undefined)delete globalThis.window;else globalThis.window=originalWindow;delete globalThis.__clearHooks;await rm(dir,{recursive:true,force:true})}
});

test('очистка из другого окна останавливает активный ответ и не трогает другое пространство',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-cross-window-'));const oldFetch=globalThis.fetch,oldWindow=globalThis.window;
 try{
  const plugin={name:'react-fixture',setup(b){
   b.onResolve({filter:/^react$/},()=>({path:'react',namespace:'fixture'}));b.onResolve({filter:/^react\/jsx-runtime$/},()=>({path:'jsx',namespace:'fixture'}));
   b.onResolve({filter:/^react-markdown$|^remark-|^rehype-/},a=>({path:a.path,namespace:'fixture'}));
   b.onResolve({filter:/^\.\/ragEvents$/},()=>({path:'events',namespace:'fixture'}));
   b.onLoad({filter:/.*/,namespace:'fixture'},a=>{
    if(a.path==='react')return {contents:`export function useState(v){let h=globalThis.__crossHooks,i=h.cursor++;if(!(i in h.values))h.values[i]=v;return [h.values[i],x=>h.values[i]=typeof x==='function'?x(h.values[i]):x]} export function useRef(v){let h=globalThis.__crossHooks,i=h.refCursor++;return h.refs[i]??=({current:v})}export function useEffect(fn){globalThis.__crossHooks.effects.push(fn)}export function useLayoutEffect(){}export function isValidElement(){return false}export default {memo:x=>x};`,loader:'js'};
    if(a.path==='jsx')return {contents:`export const Fragment=Symbol.for('fixture.fragment');export function jsx(type,props){return {type,props:props||{}}}export const jsxs=jsx;`,loader:'js'};
    if(a.path==='events')return {contents:'export async function watchRagEvents(){}',loader:'js'};
    return {contents:'export default function Stub(){}',loader:'js'};
   });
  }};
  const out=join(dir,'panel.mjs');await build({entryPoints:['src/ChatPanel.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',plugins:[plugin],loader:{'.css':'empty'},logLevel:'silent'});const {ChatPanel}=await import(pathToFileURL(out).href);
  class FakeWindow extends EventTarget{constructor(){super();this.localStorage={setItem:(key,newValue)=>{for(const w of windows)if(w!==this){const e=new Event('storage');Object.assign(e,{key,newValue});w.dispatchEvent(e)}}}}}
  const windows=[new FakeWindow(),new FakeWindow(),new FakeWindow()];let cancellations=0;
  globalThis.fetch=async(url,options)=>{if(url.includes('/rag/cancel/')){cancellations++;return {ok:true}}if(url.endsWith('/chat/send'))return await new Promise((_,reject)=>options.signal.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true}));return {ok:true,json:async()=>({messages:[],next_before:null})}};
  const instances=windows.map((w,i)=>({window:w,prefix:i===2?'/api/w/other':'/api/w/same',hooks:{cursor:0,refCursor:0,values:[[{seq:1,role:'user',content:'старая история'}], 'новый вопрос'],refs:[],effects:[]}}));
  for(const x of instances)x.hooks.values[8]=true;
  function render(x){globalThis.window=x.window;globalThis.__crossHooks=x.hooks;x.hooks.cursor=0;x.hooks.refCursor=0;x.hooks.effects=[];return ChatPanel({prefix:x.prefix,objectIds:[],visible:true,onOpen(){},readiness:null,onOpenSettings(){}})}
  const nodes=(root,pred)=>{const found=[];function visit(n){if(Array.isArray(n))return n.forEach(visit);if(!n||typeof n!=='object')return;if(pred(n))found.push(n);visit(n.props?.children)}visit(root);return found};
  const cleanups=[];for(const x of instances){render(x);cleanups.push(x.hooks.effects[2]())}
  const pending=render(instances[1]);nodes(pending,n=>n.type==='form')[0].props.onSubmit({preventDefault(){}});assert.equal(instances[1].hooks.values[5],true);
  let source=render(instances[0]);nodes(source,n=>n.type==='button'&&n.props['aria-label']==='Очистить контекст')[0].props.onClick();source=render(instances[0]);const dialog=nodes(source,n=>n.props?.role==='alertdialog')[0];nodes(dialog,n=>n.type==='button'&&n.props.children==='Очистить')[0].props.onClick();await new Promise(r=>setImmediate(r));
  assert.deepEqual(instances[0].hooks.values[0],[]);assert.deepEqual(instances[1].hooks.values[0],[]);assert.equal(instances[1].hooks.values[5],false);assert.equal(instances[1].hooks.values[1],'');assert.equal(instances[2].hooks.values[0][0].content,'старая история');assert.equal(cancellations,1);
  for(const off of cleanups)off?.();
 }finally{globalThis.fetch=oldFetch;if(oldWindow===undefined)delete globalThis.window;else globalThis.window=oldWindow;delete globalThis.__crossHooks;await rm(dir,{recursive:true,force:true})}
});
