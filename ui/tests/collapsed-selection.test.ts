import test from 'node:test';
import assert from 'node:assert/strict';
import {objectsAndStacksInArea} from '../src/selection.ts';

test('выделение свёрнутой группы включает её карточки и игнорирует их старые позиции',()=>{
 const objects=[{id:'a',x:100,y:100},{id:'b',x:450,y:100},{id:'c',x:900,y:100}];
 const stacks=[{id:'s',ids:['a','b'],collapsed:true}];
 assert.deepEqual(new Set(objectsAndStacksInArea(objects,stacks,{x:85,y:60},{x:110,y:75},{x:0,y:0},1)),new Set(['a','b']));
 assert.deepEqual(objectsAndStacksInArea(objects,stacks,{x:500,y:110},{x:510,y:120},{x:0,y:0},1),[]);
 assert.deepEqual(objectsAndStacksInArea(objects,stacks,{x:900,y:100},{x:910,y:110},{x:0,y:0},1),['c']);
});
