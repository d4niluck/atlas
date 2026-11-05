import test from 'node:test';
import assert from 'node:assert/strict';
import {objectsInArea,selectionRectangle} from '../src/selection.ts';
test('выбор пересекающихся карточек с учётом перемещения и масштаба',()=>{
 const items=[{id:'a',x:100,y:50},{id:'b',x:500,y:200}];
 assert.deepEqual(objectsInArea(items,{x:120,y:130},{x:330,y:190},{x:20,y:80},2),['a']);
 assert.deepEqual(objectsInArea(items,{x:330,y:190},{x:120,y:130},{x:20,y:80},2),['a']);
 assert.deepEqual(objectsInArea(items,{x:0,y:0},{x:10,y:10},{x:20,y:80},2),[]);
});
test('рамка строится при движении в любом направлении',()=>{
 assert.deepEqual(selectionRectangle({x:20,y:30},{x:5,y:12}),{x:5,y:12,width:15,height:18});
});

test('выделение нескольких областей добавляет карточки без повторений', async()=>{
 const {mergeSelection}=await import('../src/selection.ts');
 assert.deepEqual(mergeSelection(['a','b'],['b','c'],true),['a','b','c']);
 assert.deepEqual(mergeSelection(['a','b'],['c'],false),['c']);
 assert.deepEqual(mergeSelection(['a'],[],true),['a']);
 assert.deepEqual(mergeSelection(['a'],[],false),[]);
});

test('выделение свёрнутой группы возвращает все её карточки',async()=>{
 const {objectsAndStacksInArea}=await import('../src/selection.ts');
 const items=[{id:'a',x:100,y:100},{id:'b',x:600,y:200},{id:'c',x:900,y:700}];
 assert.deepEqual(objectsAndStacksInArea(items,[{id:'g',ids:['a','b'],collapsed:true}],{x:0,y:0},{x:400,y:150},{x:0,y:0},1),['a','b']);
});
