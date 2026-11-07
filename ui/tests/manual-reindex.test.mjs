import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

// Exercise the component's actual event handlers without a browser or model.
async function fixture(settingsOverrides={}) {
 const dir=await mkdtemp(join(process.cwd(),'.atlas-reindex-test-'));
 const plugin={name:'reindex-react-fixture',setup(b){
  b.onResolve({filter:/^react$/},()=>({path:'react',namespace:'fixture'}));
  b.onResolve({filter:/^react\/jsx-runtime$/},()=>({path:'jsx',namespace:'fixture'}));
  b.onResolve({filter:/^\.\/(SearchSettingsDialog|DeviceSettings|RagModelSettings|ModelDownload|AgenticToggle)$/},args=>({path:args.path,namespace:'fixture'}));
  b.onLoad({filter:/.*/,namespace:'fixture'},args=>{
   if(args.path==='react')return {contents:`export function useState(initial){const h=globalThis.__atlasHooks;const i=h.cursor++;if(!(i in h.values))h.values[i]=(i<2&&initial===null)?h.settings:initial;return [h.values[i],value=>{h.values[i]=typeof value==='function'?value(h.values[i]):value}]} export function useEffect(){} export default {};`,loader:'js'};
   if(args.path==='jsx')return {contents:`export const Fragment=Symbol.for('fixture.fragment');export function jsx(type,props){return {type,props:props||{}}}export const jsxs=jsx;`,loader:'js'};
   const name=args.path.slice(2);return {contents:`export function ${name}() {return null}`,loader:'js'};
  });
 }};
 try {
  const out=join(dir,'settings.mjs');
  await build({entryPoints:['src/SearchSettings.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',plugins:[plugin],loader:{'.css':'empty'},logLevel:'silent'});
  const {SearchSettings}=await import(pathToFileURL(out).href);
  const settings={agentic:false,model:'e2b',embedding_variant:'q8',embedding:'cpu',llm:'cpu',lexical_enabled:true,semantic_enabled:true,rag_enabled:true,idle_enabled:false,idle_minutes:15,image_enabled:false,ngram_enabled:false,...settingsOverrides};
  globalThis.__atlasHooks={values:[],cursor:0,settings};
  let calls=0;
  const props={onClose(){},onApply:async()=>{},installedE2b:true,installedE4b:false,installedNemotron:false,onRebuild:async()=>{calls++}};
  function render(overrides={}){globalThis.__atlasHooks.cursor=0;return SearchSettings({...props,...overrides})}
  function visit(node,fn){if(Array.isArray(node)){for(const child of node)visit(child,fn);return}if(!node||typeof node!=='object')return;fn(node);visit(node.props?.children,fn)}
  function nodes(tree,predicate){const result=[];visit(tree,node=>{if(predicate(node))result.push(node)});return result}
  function label(node){let result='';function text(child){if(Array.isArray(child))return child.forEach(text);if(typeof child==='string'||typeof child==='number')result+=child;else if(child&&typeof child==='object')text(child.props?.children)}text(node);return result}
  function button(tree,name){return nodes(tree,n=>n.type==='button'&&label(n).trim()===name)[0]}
  return {render,nodes,label,button,get calls(){return calls},dispose:async()=>{delete globalThis.__atlasHooks;await rm(dir,{recursive:true,force:true})}};
 }catch(error){await rm(dir,{recursive:true,force:true});throw error}
}

test('переиндексация доступна только в настройках пространства и скрыта до раскрытия',async()=>{
 const f=await fixture();try{
  const tree=f.render();const details=f.nodes(tree,n=>n.type==='details'&&f.label(n).includes('Переиндексация'));
  assert.equal(details.length,1,'в конце настроек должен быть свёрнутый раздел переиндексации');
  assert.notEqual(details[0].props.open,true,'раздел свёрнут по умолчанию');
  assert.ok(f.nodes(details[0],n=>n.type==='summary'&&f.label(n).includes('Переиндексация')).length,'есть управление раскрытием');
  assert.ok(f.button(details[0],'Переиндексировать'));
  assert.equal(f.calls,0);
  const home=f.render({onRebuild:undefined});
  assert.equal(f.nodes(home,n=>n.type==='details'&&f.label(n).includes('Переиндексация')).length,0,'без открытого пространства пункт скрыт');
 }finally{await f.dispose()}
});

test('переиндексация запускается только после предупреждения и подтверждения',async()=>{
 const f=await fixture();try{
  let tree=f.render();assert.ok(f.button(tree,'Переиндексировать'),'кнопка запуска');
  f.button(tree,'Переиндексировать').props.onClick();tree=f.render();
  assert.equal(f.calls,0,'открытие подтверждения не запускает операцию');
  const warning=f.nodes(tree,n=>n.type==='p'&&/переиндексац/i.test(f.label(n)));
  assert.ok(warning.length,'показано предупреждение о переиндексации');
  assert.ok(f.button(tree,'Отмена'));assert.ok(f.button(tree,'Запустить'));
  f.button(tree,'Отмена').props.onClick();tree=f.render();
  assert.equal(f.calls,0,'после отмены операция не запущена');
  f.button(tree,'Переиндексировать').props.onClick();tree=f.render();
  await f.button(tree,'Запустить').props.onClick();
  assert.equal(f.calls,1,'подтверждение запускает ровно одну операцию');
 }finally{await f.dispose()}
});

test('при выключенном смысловом поиске ручной запуск недоступен',async()=>{
 const f=await fixture({semantic_enabled:false});try{
  const tree=f.render();
  assert.equal(f.button(tree,'Переиндексировать').props.disabled,true);
  assert.ok(f.nodes(tree,n=>n.type==='p'&&/Включите смысловой поиск/.test(f.label(n))).length);
  assert.equal(f.calls,0);
 }finally{await f.dispose()}
});

test('ошибка ручного запуска показывается пользователю',async()=>{
 const f=await fixture();try{
  const onRebuild=async()=>{throw Error('Не удалось запустить переиндексацию')};
  let tree=f.render({onRebuild});
  f.button(tree,'Переиндексировать').props.onClick();tree=f.render({onRebuild});
  await f.button(tree,'Запустить').props.onClick();tree=f.render({onRebuild});
  assert.ok(f.nodes(tree,n=>n.props?.role==='alert'&&/Не удалось запустить переиндексацию/.test(f.label(n))).length);
 }finally{await f.dispose()}
});
