import test from 'node:test';
import assert from 'node:assert/strict';
import { BoardHistory } from '../src/boardHistory.ts';
const base = () => ({objects:[{id:'a',x:0,y:0,title:'current'},{id:'b',x:20,y:10,title:'b'}],edges:[],stacks:[]});
test('undo restores positions and preserves current note metadata', () => {
 const h = new BoardHistory(); const a=base(); const b={...a,objects:a.objects.map(o=>o.id==='a'?{...o,x:20}:o)};
 h.record(a,b); const edited={...b,objects:b.objects.map(o=>o.id==='a'?{...o,title:'updated'}:o)};
 const previous=h.undo(edited); assert.equal(previous?.objects[0].x,0); assert.equal(previous?.objects[0].title,'updated');
});
test('does not overwrite independent board changes or create/delete objects', () => {
 const h=new BoardHistory(); const a=base(); const b={...a,edges:[{id:'e',from:'a',to:'b'}]}; h.record(a,b);
 assert.equal(h.undo({...b,objects:[...b.objects,{id:'c',x:0,y:0,title:'new'}]}),null);
});
test('history resets when object membership changes', () => {
 const h=new BoardHistory();const a=base();const b={...a,objects:a.objects.map(o=>({...o,x:10}))};h.record(a,b);
 h.record(b,{...b,objects:b.objects.slice(1)});assert.equal(h.undo(b),null);
});
test('object creation produces guarded undo plan and is not consumed until confirmed', () => {
 const h=new BoardHistory();const before=base();const after={...before,objects:[...before.objects,{id:'c',x:2,y:3,title:'c'}]};
 h.recordCreated(before,after,'c','valid-revision');
 assert.deepEqual(h.prepareUndo(after),{kind:'created',id:'c',revision:'valid-revision'});
 assert.deepEqual(h.prepareUndo(after),{kind:'created',id:'c',revision:'valid-revision'});
 h.confirmUndo();assert.equal(h.prepareUndo(before),null);
});
test('object deletion guarded by exact board state and workspace-owned history', () => {
 const h=new BoardHistory(),other=new BoardHistory();const before=base();const after={...before,objects:before.objects.slice(1)};
 h.recordDeleted(before,after,'a');assert.deepEqual(h.prepareUndo(after),{kind:'deleted',id:'a'});assert.equal(other.prepareUndo(after),null);
 assert.equal(h.prepareUndo({...after,objects:[...after.objects,{id:'c',x:0,y:0,title:'other'}]}),null);
});
test('group delete provides a single atomic undo plan',()=>{
 const h=new BoardHistory();const before=base();const after={...before,objects:[]};
 h.recordDeletedGroup(before,after,['a','b']);
 assert.deepEqual(h.prepareUndo(after),{kind:'deleted-group',ids:['a','b']});
 assert.deepEqual(h.prepareUndo(after),{kind:'deleted-group',ids:['a','b']});
 h.confirmUndo();assert.equal(h.prepareUndo(before),null);
});
test('undo restores image preview state',()=>{
 const history=new BoardHistory();
 const before={objects:[{id:'image',x:0,y:0,imageExpanded:false}],edges:[],stacks:[]};
 const after={...before,objects:[{...before.objects[0],imageExpanded:true}]};
 history.record(before,after);
 assert.equal(history.undo(after)?.objects[0].imageExpanded,false);
});
