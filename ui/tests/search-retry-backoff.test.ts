import {strict as assert} from 'node:assert';
import {searchWithRetry} from '../src/searchRetry.ts';
const originalFetch=globalThis.fetch;const originalTimeout=globalThis.setTimeout;
const delays:number[]=[];let calls=0;let waiting=0;
globalThis.setTimeout=((fn:()=>void,ms:number)=>{delays.push(ms);queueMicrotask(fn);return 1}) as typeof setTimeout;
globalThis.fetch=async()=>{calls++;return calls<=6?new Response(JSON.stringify({error:'Подготавливаем индекс поиска. Попробуйте ещё раз через несколько секунд.'}),{status:503}):new Response('{}',{status:200})};
try{assert.equal((await searchWithRetry('/search',{},()=>waiting++)).status,200);assert.deepEqual(delays,[250,500,1000,1500,1500,1500]);assert.equal(waiting,6);}finally{globalThis.fetch=originalFetch;globalThis.setTimeout=originalTimeout;}
