import test from 'node:test';
import assert from 'node:assert/strict';
import {extractionPosition} from '../src/fitBoard.ts';

test('extracts to the right of the widest remaining card',()=>{
 const objects=[{id:'wide',x:100,y:100,width:920,height:180},{id:'out',x:150,y:300,width:230,height:82}];
 const pos=extractionPosition(objects,[{id:'g',ids:['wide','out'],collapsed:false}],'out');
 assert.ok(pos);assert.ok(pos.x>=1060);assert.equal(pos.y,100);
});

test('avoids a neighbouring card using extracted image dimensions',()=>{
 const objects=[{id:'keep',x:0,y:0,width:230,height:82},{id:'out',x:0,y:130,width:920,height:400},{id:'other',x:800,y:0,width:230,height:82}];
 const pos=extractionPosition(objects,[{id:'g',ids:['keep','out'],collapsed:false}],'out');
 assert.ok(pos);assert.equal(pos.x,270);assert.ok(pos.y>=102);
});

test('extracts the final card in place and rejects missing groups',()=>{
 assert.deepEqual(extractionPosition([{id:'a',x:12,y:34}],[{id:'g',ids:['a']}],'a'),{x:12,y:34});
 assert.equal(extractionPosition([{id:'a',x:0,y:0}],[],'a'),null);
});

test('ignores hidden cards of a collapsed group outside its visible frame',()=>{
 const objects=[{id:'keep',x:0,y:0},{id:'out',x:0,y:130},{id:'anchor',x:0,y:0},{id:'hidden',x:270,y:0}];
 const stacks=[{id:'source',ids:['keep','out'],collapsed:false},{id:'closed',ids:['anchor','hidden'],collapsed:true}];
 assert.deepEqual(extractionPosition(objects,stacks,'out'),{x:270,y:0});
});

test('still avoids the visible frame of a collapsed group',()=>{
 const objects=[{id:'keep',x:0,y:0},{id:'out',x:0,y:130},{id:'anchor',x:300,y:0},{id:'hidden',x:300,y:130}];
 const stacks=[{id:'source',ids:['keep','out'],collapsed:false},{id:'closed',ids:['anchor','hidden'],collapsed:true}];
 assert.deepEqual(extractionPosition(objects,stacks,'out'),{x:270,y:120});
});
