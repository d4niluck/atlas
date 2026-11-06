import test from 'node:test';
import assert from 'node:assert/strict';
import {stackLayers,stackDropPriority,isForegroundEdge,isCrossingEdge,stackFocusForObject,stackFocusAfterDrop} from '../src/stackLayers.ts';

test('expanded group is raised with its cards, but collapsed cards are not',()=>{
 const result=stackLayers([{id:'one',ids:['a','b'],collapsed:false},{id:'two',ids:['c'],collapsed:true}],null);
 assert.equal(result.active,'one');assert.deepEqual([...result.grouped],['a','b']);assert.deepEqual([...result.foreground],['a','b']);
});
test('the selected expanded group is foremost, and switches without changing memberships',()=>{
 const groups=[{id:'one',ids:['a'],collapsed:false},{id:'two',ids:['b'],collapsed:false}];
 const first=stackLayers(groups,'one'), second=stackLayers(groups,'two');
 assert.equal(first.active,'one');assert.deepEqual([...first.foreground],['a']);
 assert.equal(second.active,'two');assert.deepEqual([...second.foreground],['b']);
 assert.deepEqual([...second.grouped],['a','b']);
});
test('closed or deleted focus falls back to the last open group, or none',()=>{
 assert.equal(stackLayers([{id:'one',ids:['a'],collapsed:true},{id:'two',ids:['b'],collapsed:false}],'one').active,'two');
 assert.equal(stackLayers([{id:'one',ids:['a'],collapsed:true}],'one').active,null);
});
test('drop prefers the active open group over other open and collapsed groups',()=>{
 assert.ok(stackDropPriority({id:'a',ids:[],collapsed:false},'a')>stackDropPriority({id:'b',ids:[],collapsed:false},'a'));
 assert.ok(stackDropPriority({id:'b',ids:[],collapsed:false},'a')>stackDropPriority({id:'c',ids:[],collapsed:true},'a'));
});
test('only edges wholly inside the active group are raised above its blurred background',()=>{
 const members=new Set(['a','b']);
 assert.equal(isForegroundEdge({from:'a',to:'b'},members),true);
 assert.equal(isForegroundEdge({from:'a',to:'c'},members),false);
});
test('searching for an object focuses its group even when it was collapsed',()=>{
 const groups=[{id:'first',ids:['a'],collapsed:false},{id:'second',ids:['b'],collapsed:true}];
 assert.equal(stackFocusForObject(groups,'b','first'),'second');
 assert.equal(stackFocusForObject(groups,'outside','first'),'first');
});
test('dropping into an open group focuses it without opening a collapsed group',()=>{
 const groups=[{id:'first',ids:['a'],collapsed:false},{id:'second',ids:['b'],collapsed:true},{id:'third',ids:['c'],collapsed:false}];
 assert.equal(stackFocusAfterDrop(groups,'third','first'),'third');
 assert.equal(stackFocusAfterDrop(groups,'second','first'),'first');
 assert.equal(stackFocusAfterDrop(groups,undefined,'first'),'first');
});
test('inter-group edges are identified separately for clipped foreground drawing',()=>{
 const members=new Set(['a','b']);
 assert.equal(isCrossingEdge({from:'a',to:'b'},members),false);
 assert.equal(isCrossingEdge({from:'a',to:'c'},members),true);
 assert.equal(isCrossingEdge({from:'c',to:'a'},members),true);
 assert.equal(isCrossingEdge({from:'c',to:'d'},members),false);
});
