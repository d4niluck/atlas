import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('после очистки переписка и возможность загрузить старые страницы исчезают',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-chat-clear-test-'));const originalFetch=globalThis.fetch;
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
  let requests=[],allowClear=false;globalThis.fetch=async(url,options)=>{requests.push({url,method:options?.method});return {ok:allowClear,json:async()=>({messages:[],next_before:null})}};
  const render=()=>{globalThis.__clearHooks.cursor=0;globalThis.__clearHooks.refCursor=0;return ChatPanel({prefix:'/api/w/test',objectIds:[],visible:true,onOpen(){},readiness:null,onOpenSettings(){}})};
  const nodes=(root,pred)=>{const found=[];function visit(n){if(Array.isArray(n))return n.forEach(visit);if(!n||typeof n!=='object')return;if(pred(n))found.push(n);visit(n.props?.children)}visit(root);return found};
  let tree=render();nodes(tree,n=>n.type==='button'&&n.props['aria-label']==='Очистить контекст')[0].props.onClick();tree=render();
  let dialog=nodes(tree,n=>n.props?.role==='alertdialog')[0];assert.ok(dialog);nodes(dialog,n=>n.type==='button'&&n.props.children==='Очистить')[0].props.onClick();await new Promise(r=>setImmediate(r));tree=render();assert.equal(globalThis.__clearHooks.values[0][0].content,'старый вопрос','ошибка удаления не очищает историю');assert.equal(globalThis.__clearHooks.values[9],1);
  allowClear=true;dialog=nodes(tree,n=>n.props?.role==='alertdialog')[0];nodes(dialog,n=>n.type==='button'&&n.props.children==='Очистить')[0].props.onClick();await new Promise(r=>setImmediate(r));tree=render();
  assert.deepEqual(globalThis.__clearHooks.values[0],[]);assert.equal(globalThis.__clearHooks.values[9],null);assert.equal(requests[0].method,'POST');
 }finally{globalThis.fetch=originalFetch;delete globalThis.__clearHooks;await rm(dir,{recursive:true,force:true})}
});
