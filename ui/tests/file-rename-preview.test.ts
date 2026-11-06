import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';

// Exercise the actual controller with a preview open, without a browser dependency.
const source=readFileSync(new URL('../src/main.tsx',import.meta.url),'utf8');
const start=source.indexOf(' async function renameFile('),end=source.indexOf('\n }',start);
assert.ok(start>=0&&end>start,'rename handler not found');
const handler=stripTypeScriptTypes(source.slice(start,end+3),{mode:'strip'});

function setup(responseLost:boolean){
 let preview:any={id:'file-id',title:'old.pdf',kind:'file'};
 const boardRef={current:{objects:[{id:'file-id',title:'old.pdf',kind:'file',x:7,y:9}],edges:[],stacks:[]}};
 const appRef={current:{inert:false}},busy={current:false},closingRef={current:false};
 const api=async()=>{if(responseLost)throw Error('network');return {id:'file-id',title:'new.pdf'}};
 const sync=async()=>{boardRef.current.objects[0].title='new.pdf';return true};
 const create=new Function('api','flushBoard','body','boardRef','setBoard','setPreview','appRef','busy','setUndoing','closingRef','syncAfterFailure',`return (${handler})`);
 const rename=create(api,async()=>{},JSON.stringify,boardRef,()=>{},(update:any)=>{preview=update(preview)},appRef,busy,()=>{},closingRef,sync);
 return {rename,boardRef,appRef,busy,getPreview:()=>preview};
}

test('renaming updates an open file preview and retains position',async()=>{
 const s=setup(false);await s.rename('file-id','new.pdf');
 assert.equal(s.getPreview().title,'new.pdf');assert.equal(s.boardRef.current.objects[0].x,7);assert.equal(s.appRef.current.inert,false);assert.equal(s.busy.current,false);
});

test('a lost response resynchronizes an open preview with the saved title',async()=>{
 const s=setup(true);await assert.rejects(s.rename('file-id','new.pdf'),/network/);
 assert.equal(s.getPreview().title,'new.pdf');assert.equal(s.appRef.current.inert,false);assert.equal(s.busy.current,false);
});
