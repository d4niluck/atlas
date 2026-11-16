import {test} from 'node:test';
import assert from 'node:assert/strict';
import {acquireBoardFlush} from '../src/boardFlush.ts';

test('four concurrent flushes never overlap and observe prior changes',async()=>{
 const ref:{current:Promise<void>|null}={current:null};let active=0;let completed=0;let openFirst!:()=>void;
 const gate=new Promise<void>(resolve=>{openFirst=resolve});const seen:number[]=[];
 const run=async(first=false)=>{const release=await acquireBoardFlush(ref);try{assert.equal(++active,1);seen.push(completed);if(first)await gate;await Promise.resolve();completed++;active--}finally{release()}};
 const first=run(true);await Promise.resolve();const waiting=[run(),run(),run()];await Promise.resolve();assert.equal(active,1);openFirst();await Promise.all([first,...waiting]);assert.deepEqual(seen,[0,1,2,3]);assert.equal(ref.current,null);
});
