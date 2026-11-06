import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createViewport} from '../src/viewportController.ts';
import {pastePosition} from '../src/pastePosition.ts';
import {objectsInArea} from '../src/selection.ts';

test('paste and selection during pending wheel gesture use displayed coordinates',()=>{
 let live:any; let pending:(()=>void)|undefined; let commits=0;
 const v=createViewport({offset:{x:0,y:0},zoom:1},x=>live=x,()=>commits++,((f:()=>void)=>{pending=f;return 1}) as any,(()=>{pending=undefined}) as any);
 v.pan(160,-20); v.zoomAt({x:100,y:100},2); v.wheelEnd();
 assert.equal(commits,0); assert.deepEqual(live,v.get());
 const placed=pastePosition({clientWidth:900,clientHeight:600},live);
 assert.equal(placed.x*live.zoom+live.offset.x,(900-230*live.zoom)/2);
 assert.equal(placed.y*live.zoom+live.offset.y,(600-82*live.zoom)/2);
 assert.deepEqual(objectsInArea([{id:'x',x:placed.x,y:placed.y}],{x:0,y:0},{x:900,y:600},live.offset,live.zoom),['x']);
 pending?.(); assert.equal(commits,1);
});
test('independent workspace state survives a pending commit and disposal',()=>{
 const timers=new Map<number,()=>void>(); let n=0;const commitsA:any[]=[],commitsB:any[]=[];
 const schedule=((f:()=>void)=>{timers.set(++n,f);return n}) as any;const cancel=((id:number)=>timers.delete(id)) as any;
 const a=createViewport({offset:{x:0,y:0},zoom:1},()=>{},v=>commitsA.push(v),schedule,cancel);
 const b=createViewport({offset:{x:300,y:400},zoom:2},()=>{},v=>commitsB.push(v),schedule,cancel);
 a.pan(15,30);a.wheelEnd();b.pan(10,-20);b.wheelEnd();a.dispose();
 assert.equal(timers.size,1); [...timers.values()][0]();
 assert.deepEqual(commitsA,[]); assert.deepEqual(commitsB,[{offset:{x:310,y:380},zoom:2}]);
});
test('pointer cancellation finalizes movement and invalidates delayed wheel commit',()=>{
 const timers=new Map<number,()=>void>();let n=0;const commits:any[]=[];
 const v=createViewport({offset:{x:2,y:3},zoom:1},()=>{},x=>commits.push(x),((f:()=>void)=>{timers.set(++n,f);return n}) as any,((id:number)=>timers.delete(id)) as any);
 v.pan(5,6);v.wheelEnd();v.movePointer({x:7,y:9},25,40);v.finish();
 assert.equal(timers.size,0);assert.deepEqual(commits,[{offset:{x:32,y:49},zoom:1}]);
});
test('wheel, fit, and an immediate new gesture retain fit zoom and coordinates',()=>{
 const timers=new Map<number,()=>void>();let n=0;const commits:any[]=[];
 const v=createViewport({offset:{x:0,y:0},zoom:1},()=>{},x=>commits.push(x),((f:()=>void)=>{timers.set(++n,f);return n}) as any,((id:number)=>timers.delete(id)) as any);
 v.pan(15,-20);v.wheelEnd();v.set({offset:{x:-100,y:250},zoom:0.25});v.pan(8,3);v.wheelEnd();
 assert.equal(timers.size,1);[...timers.values()][0]();assert.deepEqual(commits.at(-1),{offset:{x:-92,y:253},zoom:0.25});
});
test('continuous pan paints each delta without committing React state per event',()=>{
 let paints=0,commits=0,pending:(()=>void)|undefined;
 const v=createViewport({offset:{x:0,y:0},zoom:1},()=>paints++,()=>commits++,((f:()=>void)=>{pending=f;return 1}) as any,(()=>{pending=undefined}) as any);
 for(let i=0;i<100;i++){v.pan(1,-2);v.wheelEnd()}
 assert.equal(paints,100);assert.equal(commits,0);assert.deepEqual(v.get(),{offset:{x:100,y:-200},zoom:1});
 pending?.();assert.equal(commits,1);
});
