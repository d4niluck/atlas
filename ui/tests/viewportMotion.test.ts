import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createViewport } from '../src/viewportController.ts';

function setup() {
 const states:boolean[]=[]; const pending=new Map<number,()=>void>(); let id=0;
 const viewport=createViewport({offset:{x:0,y:0},zoom:1},()=>{},()=>{},(fn)=>{pending.set(++id,fn);return id as any},(timer)=>{pending.delete(timer as any)},moving=>states.push(moving));
 return {viewport,states,pending,flush(){for(const fn of [...pending.values()])fn();pending.clear()}};
}
test('wheel and zoom keep blur disabled until inactivity',()=>{const t=setup();t.viewport.pan(1,2);t.viewport.wheelEnd();t.viewport.zoomAt({x:4,y:5},1.1);t.viewport.wheelEnd();assert.deepEqual(t.states,[true]);t.flush();assert.deepEqual(t.states,[true,false]);});
test('pointer finish restores blur',()=>{const t=setup();t.viewport.movePointer({x:0,y:0},10,10);t.viewport.finish();assert.deepEqual(t.states,[true,false]);});
test('programmatic transition cancels motion and stale timer',()=>{const t=setup();t.viewport.pan(1,2);t.viewport.wheelEnd();t.viewport.set({offset:{x:5,y:6},zoom:2});t.flush();assert.deepEqual(t.states,[true,false]);});
test('dispose restores blur and cancels timer',()=>{const t=setup();t.viewport.pan(1,2);t.viewport.wheelEnd();t.viewport.dispose();t.flush();assert.deepEqual(t.states,[true,false]);});

test('pointer motion cancels earlier wheel idle timer',()=>{const t=setup();t.viewport.pan(1,2);t.viewport.wheelEnd();t.viewport.movePointer({x:1,y:2},5,5);t.flush();assert.deepEqual(t.states,[true]);t.viewport.finish();assert.deepEqual(t.states,[true,false]);});

test('wheel idle while pointer is held keeps motion until pointer finish',()=>{const t=setup();t.viewport.beginPointer();t.viewport.movePointer({x:0,y:0},2,3);t.viewport.pan(1,1);t.viewport.wheelEnd();t.flush();assert.deepEqual(t.states,[true]);t.viewport.finish();assert.deepEqual(t.states,[true,false]);});
