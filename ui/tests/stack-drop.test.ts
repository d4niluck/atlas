import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';

// Test the exact drop handler used by the canvas without introducing a DOM harness.
const source=readFileSync(new URL('../src/main.tsx',import.meta.url),'utf8');
const match=source.match(/^\s*(function addCardsToStack\([^\n]+)$/m);
assert.ok(match, 'canvas drop handler was not found');
const handler=stripTypeScriptTypes(match[1],{mode:'strip'});
const addCardsToStack=new Function(`return (${handler})`)();

test('adding cards keeps an expanded group expanded',()=>{
 const stacks=[{id:'a',title:'Группа',ids:['one'],collapsed:false}];
 assert.deepEqual(addCardsToStack(stacks,'a',['two']),[{id:'a',title:'Группа',ids:['one','two'],collapsed:false}]);
});

test('adding cards keeps a collapsed group collapsed',()=>{
 const stacks=[{id:'a',title:'Группа',ids:['one'],collapsed:true}];
 assert.deepEqual(addCardsToStack(stacks,'a',['two']),[{id:'a',title:'Группа',ids:['one','two'],collapsed:true}]);
});

test('adding cards preserves the default expanded state and moves them from another group',()=>{
 const stacks=[{id:'a',title:'Первая',ids:['one']},{id:'b',title:'Вторая',ids:['two','three'],collapsed:true}];
 assert.deepEqual(addCardsToStack(stacks,'a',['two','two']),[
  {id:'a',title:'Первая',ids:['one','two']},
  {id:'b',title:'Вторая',ids:['three'],collapsed:true}
 ]);
});
