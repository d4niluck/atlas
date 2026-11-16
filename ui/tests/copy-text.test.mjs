import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('copy uses Clipboard API or falls back to selected text and reports failure',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-copy-test-'));
 const previousNavigator=Object.getOwnPropertyDescriptor(globalThis,'navigator');
 const previousDocument=Object.getOwnPropertyDescriptor(globalThis,'document');
 try{
  const out=join(dir,'copy.mjs');
  await build({entryPoints:['src/copyText.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});
  const {copyText}=await import(pathToFileURL(out).href);
  let copied='',selected=false,removed=false,success=true;
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{clipboard:{writeText:async text=>{copied=text}}}});
  await copyText('first');assert.equal(copied,'first');
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{clipboard:{writeText:async()=>{throw Error('denied')}}}});
  Object.defineProperty(globalThis,'document',{configurable:true,value:{createElement:()=>({style:{},setAttribute(){},select(){selected=true},remove(){removed=true}}),body:{appendChild(){}},execCommand:()=>success}});
  await copyText('second');assert.ok(selected&&removed);
  success=false;removed=false;await assert.rejects(copyText('third'));assert.ok(removed);
  Object.defineProperty(globalThis,'navigator',{configurable:true,value:{}});
  success=true;await copyText('fourth');
 }finally{
  if(previousNavigator)Object.defineProperty(globalThis,'navigator',previousNavigator);else delete globalThis.navigator;
  if(previousDocument)Object.defineProperty(globalThis,'document',previousDocument);else delete globalThis.document;
  await rm(dir,{recursive:true,force:true});
 }
});
