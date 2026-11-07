import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('пока настройки загружаются, окно не выглядит пустым',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-settings-test-'));
 try{
  const plugin={name:'settings-react-fixture',setup(b){
   b.onResolve({filter:/^react$/},()=>({path:'react',namespace:'fixture'}));
   b.onResolve({filter:/^react\/jsx-runtime$/},()=>({path:'jsx',namespace:'fixture'}));
   b.onResolve({filter:/^\.\/(SearchSettingsDialog|DeviceSettings|RagModelSettings|ModelDownload|AgenticToggle)$/},a=>({path:a.path,namespace:'fixture'}));
   b.onLoad({filter:/.*/,namespace:'fixture'},a=>{
    if(a.path==='react')return {contents:`export function useState(v){return [v,()=>{}]} export function useEffect(){} export default {};`,loader:'js'};
    if(a.path==='jsx')return {contents:`export const Fragment=Symbol.for('fixture.fragment');export function jsx(type,props){return {type,props:props||{}}}export const jsxs=jsx;`,loader:'js'};
    return {contents:`export function ${a.path.slice(2)}() {return null}`,loader:'js'};
   });
  }};
  const out=join(dir,'settings.mjs');
  await build({entryPoints:['src/SearchSettings.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',plugins:[plugin],loader:{'.css':'empty'},logLevel:'silent'});
  const {SearchSettings}=await import(pathToFileURL(out).href);
  const tree=SearchSettings({onClose(){},onApply:async()=>{},installedE2b:false,installedE4b:false,installedNemotron:false});
  const found=[];
  function visit(n){if(Array.isArray(n))return n.forEach(visit);if(!n||typeof n!=='object')return;if(n.props?.role==='status')found.push(n);visit(n.props?.children)}
  visit(tree);
  assert.ok(found.some(n=>/Загружаем настройки поиска/.test(String(n.props.children))), 'ожидание настроек должно быть видно');
 }finally{await rm(dir,{recursive:true,force:true})}
});
