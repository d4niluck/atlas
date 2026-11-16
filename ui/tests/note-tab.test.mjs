import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('Tab inserts two spaces at the selection and preserves browser undo when available',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-note-tab-'));const previousDocument=globalThis.document,previousFrame=globalThis.requestAnimationFrame;
 try{
  const out=join(dir,'editor.mjs');await build({entryPoints:['src/MarkdownEditor.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',external:['react','react-dom'],loader:{'.css':'empty'},logLevel:'silent'});
  const {insertTab}=await import(pathToFileURL(out).href);let calls=[],value,selection;const element={selectionStart:2,selectionEnd:4,setSelectionRange:(a,b)=>{selection=[a,b]}};let prevented=false;
  const event={key:'Tab',shiftKey:false,ctrlKey:false,metaKey:false,altKey:false,currentTarget:element,preventDefault(){prevented=true}};
  globalThis.document={execCommand:(...args)=>{calls.push(args);return true}};globalThis.requestAnimationFrame=fn=>fn();
  insertTab(event,'abcdef',v=>value=v);assert.equal(prevented,true);assert.deepEqual(calls,[['insertText',false,'  ']]);assert.equal(value,undefined);
  globalThis.document.execCommand=()=>false;insertTab(event,'abcdef',v=>value=v);assert.equal(value,'ab  ef');assert.deepEqual(selection,[4,4]);
  prevented=false;insertTab({...event,shiftKey:true},'abcdef',()=>assert.fail());assert.equal(prevented,false);
 }finally{globalThis.document=previousDocument;globalThis.requestAnimationFrame=previousFrame;await rm(dir,{recursive:true,force:true})}
});
