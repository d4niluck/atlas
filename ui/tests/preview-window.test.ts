import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import {readPreviewWindow} from '../src/preview-window.ts';
test('window offsets use Unicode code points across streamed emoji', async()=>{
 const text='😀'.repeat(18000)+'a'.repeat(55000)+'НАЙДЕННЫЙ ФРАГМЕНТ'+'z'.repeat(75000);
 const start=Array.from(text.slice(0,text.indexOf('НАЙДЕННЫЙ'))).length;
 const response=new Response(new TextEncoder().encode(text));
 const part=await readPreviewWindow(response,start);
 assert.equal(Array.from(part.text).slice(start-part.offset,start-part.offset+9).join(''),'НАЙДЕННЫЙ');
});
test('small files stay whole even when result is near the end', async()=>{
 const text='a'.repeat(100000)+'target';
 const part=await readPreviewWindow(new Response(new TextEncoder().encode(text)),100000);
 assert.equal(part.offset,0);assert.equal(part.truncated,false);assert.equal(part.text,text);
});
