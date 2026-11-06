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
test('undo restores image width while preserving current metadata',()=>{
 const history=new BoardHistory();
 const before={objects:[{id:'image',x:0,y:0,imageExpanded:true,imageWidth:230,title:'old'}],edges:[],stacks:[]};
 const after={...before,objects:[{...before.objects[0],imageWidth:460}]};
 history.record(before,after);
 const renamed={...after,objects:[{...after.objects[0],title:'new'}]};
 const restored=history.undo(renamed);
 assert.equal(restored?.objects[0].imageWidth,230);
 assert.equal(restored?.objects[0].title,'new');
});

test('cancelled drag restores only dragged positions and preserves earlier undo',async()=>{
 const {restoreDragPositions}=await import('../src/boardHistory.ts');
 const h=new BoardHistory();const initial=base();const saved={...initial,objects:initial.objects.map(o=>o.id==='a'?{...o,x:12}:o)};h.record(initial,saved);
 const preview={...saved,objects:saved.objects.map(o=>o.id==='a'?{...o,x:40,y:50,title:'renamed'}:o)};
 const cancelled=restoreDragPositions(preview,{a:{x:12,y:0}});
 assert.deepEqual(cancelled.objects,[{id:'a',x:12,y:0,title:'renamed'},{id:'b',x:20,y:10,title:'b'}]);
 const undo=h.undo(cancelled);assert.equal(undo?.objects[0].x,0);assert.equal(undo?.objects[0].title,'renamed');
});
test('cancelled multi-card drag restores coordinates without undoing other data',async()=>{
 const {restoreDragPositions}=await import('../src/boardHistory.ts');
 const current={objects:[{id:'a',x:90,y:100,title:'new'},{id:'b',x:130,y:140,title:'b'},{id:'c',x:5,y:6,title:'other'}],edges:[{id:'edge'}],stacks:[{id:'stack'}]};
 const result=restoreDragPositions(current,{a:{x:1,y:2},b:{x:3,y:4}});
 assert.deepEqual(result.objects.map(({x,y})=>({x,y})),[{x:1,y:2},{x:3,y:4},{x:5,y:6}]);
 assert.equal(result.objects[0].title,'new');assert.deepEqual(result.edges,current.edges);assert.deepEqual(result.stacks,current.stacks);
});

test('independent change during drag saves original coordinates and remains undoable after cancellation',async()=>{
 const {dragSafeSnapshots}=await import('../src/boardHistory.ts');
 const h=new BoardHistory();const baseBoard=base();
 const preview={...baseBoard,objects:baseBoard.objects.map(o=>o.id==='a'?{...o,x:77,y:33}:o)};
 const edited={...preview,edges:[{id:'new-edge',from:'a',to:'b'}]};
 const snapshots=dragSafeSnapshots(preview,edited,{a:{x:0,y:0}});
 h.record(snapshots.before,snapshots.after);
 assert.deepEqual(snapshots.after.objects.map(({x,y})=>({x,y})),[{x:0,y:0},{x:20,y:10}]);
 assert.deepEqual(snapshots.after.edges,edited.edges);
 const restored=h.undo(snapshots.after);assert.deepEqual(restored?.edges,[]);assert.equal(restored?.objects[0].x,0);
 assert.equal(edited.objects[0].x,77,'rendered preview stays intact');
});

test('completing a drag after independent change preserves the independent change in undo',async()=>{
 const {restoreDragPositions}=await import('../src/boardHistory.ts');
 const start=base();const withEdge={...start,edges:[{id:'e',from:'a',to:'b'}]};
 const preview={...withEdge,objects:withEdge.objects.map(o=>o.id==='a'?{...o,x:60,y:30}:o)};
 const h=new BoardHistory();h.record(start,withEdge);h.record(restoreDragPositions(preview,{a:{x:0,y:0}}),preview);
 const undoDrag=h.undo(preview);assert.equal(undoDrag?.objects[0].x,0);assert.deepEqual(undoDrag?.edges,withEdge.edges);
 const undoEdge=h.undo(undoDrag!);assert.deepEqual(undoEdge?.edges,[]);
});

test('creation finishing during a cancelled drag remains undoable',async()=>{
 const {dragSafeSnapshots}=await import('../src/boardHistory.ts');const h=new BoardHistory();const before=base();
 const preview={...before,objects:before.objects.map(o=>o.id==='a'?{...o,x:100,y:100}:o)};
 const added={...preview,objects:[...preview.objects,{id:'c',x:8,y:9,title:'created'}]};
 const safe=dragSafeSnapshots(preview,added,{a:{x:0,y:0}});
 h.recordCreated(safe.before,safe.after,'c','revision');
 assert.deepEqual(h.prepareUndo(safe.after),{kind:'created',id:'c',revision:'revision'});
});
