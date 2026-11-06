import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import {protectedSessionUnlocked, pendingPasteError} from '../src/menuWorkspaceSafety.ts';

test('only a successful backend session check unlocks ordinary protected opening',async()=>{
 assert.equal(await protectedSessionUnlocked('a',async()=>new Response('{}',{status:200})),true);
 assert.equal(await protectedSessionUnlocked('a',async()=>new Response('{}',{status:423})),false);
 assert.equal(await protectedSessionUnlocked('a',async()=>new Response('{}',{status:403})),false);
 await assert.rejects(protectedSessionUnlocked('a',async()=>new Response('{}',{status:500})),/Не удалось/);
});
test('exit refuses pending or unconfirmed pastes',()=>{
 assert.match(pendingPasteError(new Map([['a',1]]),new Set())!,/Дождитесь/);
 assert.match(pendingPasteError(new Map(),new Set(['a']))!,/подтвердить/);
 assert.equal(pendingPasteError(new Map(),new Set()),null);
});
