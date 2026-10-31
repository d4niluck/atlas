import test from 'node:test';
import assert from 'node:assert/strict';
import {stackSelection} from '../src/selection.ts';
test('собирает выбранные карточки в группу без дублирования и не трогает остальные',()=>{
 const result=stackSelection([{id:'old',title:'Старая',ids:['a','b','c']}],['a','b'],'new');
 assert.deepEqual(result,[{id:'old',title:'Старая',ids:['c']},{id:'new',title:'Группа',ids:['a','b']}]);
});
test('при одной карточке группа не создаётся',()=>{
 const original=[{id:'old',title:'Старая',ids:['a','b']}];
 assert.deepEqual(stackSelection(original,['a'],'new'),original);
});
