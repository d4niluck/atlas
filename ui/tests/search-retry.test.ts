import {strict as assert} from 'node:assert';
import {searchWithRetry} from '../src/searchRetry.ts';
const original=globalThis.fetch;
let calls=0,waiting=0;
globalThis.fetch=async()=>{calls++;return calls===1?new Response(JSON.stringify({error:'Подготавливаем индекс поиска. Попробуйте ещё раз через несколько секунд.'}),{status:503}):new Response('{}',{status:200})};
assert.equal((await searchWithRetry('/search',{},()=>waiting++)).status,200);assert.equal(calls,2);assert.equal(waiting,1);
calls=0;globalThis.fetch=async()=>{calls++;return new Response(JSON.stringify({error:'Другая ошибка'}),{status:503})};assert.equal((await searchWithRetry('/search',{},()=>{throw Error('unexpected retry')})).status,503);assert.equal(calls,1);
const controller=new AbortController();globalThis.fetch=async()=>new Response(JSON.stringify({error:'Подготавливаем индекс поиска. Попробуйте ещё раз через несколько секунд.'}),{status:503});await assert.rejects(searchWithRetry('/search',{signal:controller.signal},()=>controller.abort()),{name:'AbortError'});globalThis.fetch=original;
