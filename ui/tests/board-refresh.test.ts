import {test} from 'node:test';
import assert from 'node:assert/strict';
import {canApplyRemoteBoard} from '../src/boardFlush.ts';
test('remote board cannot replace a changed, dirty, busy or dragged local board',()=>{
 const clean={startedRevision:'a',currentRevision:'a',remoteRevision:'b',dirty:false,pending:false,busy:false,dragging:false,conflict:false};
 assert.equal(canApplyRemoteBoard(clean),true);
 for(const key of ['dirty','pending','busy','dragging','conflict'] as const)assert.equal(canApplyRemoteBoard({...clean,[key]:true}),false);
 assert.equal(canApplyRemoteBoard({...clean,currentRevision:'new'}),false);
 assert.equal(canApplyRemoteBoard({...clean,remoteRevision:'a'}),false);
});

test('a full board is fetched once at startup and again only when agent signal changes',async()=>{
 const {shouldFetchRemoteBoard}=await import('../src/boardFlush.ts');
 assert.equal(shouldFetchRemoteBoard(null,0),true);
 assert.equal(shouldFetchRemoteBoard(0,0),false);
 assert.equal(shouldFetchRemoteBoard(0,1),true);
 assert.equal(shouldFetchRemoteBoard(1,1),false);
});

test('a remotely deleted preview closes only after its description is saved',async()=>{
 const {remotePreviewAction}=await import('../src/boardFlush.ts');
 assert.equal(remotePreviewAction('a',['b'],true),'defer');
 assert.equal(remotePreviewAction('a',['b'],false),'close');
 assert.equal(remotePreviewAction('a',['a','b'],false),'keep');
 assert.equal(remotePreviewAction(null,['b'],false),'keep');
});
