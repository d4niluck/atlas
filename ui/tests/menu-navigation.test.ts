import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import {createMenuNavigation} from '../src/menuNavigation.ts';

const source = {workspaceId:'workspace-a',id:'note-a',chunk:{text:'нужный фрагмент',start:1,end:17}};

test('request survives until the matching board is loaded', async()=>{
 const opened:string[]=[];
 const nav=createMenuNavigation({openWorkspace:async id=>{opened.push(id);return true}});
 await nav.request(source);
 assert.deepEqual(opened,['workspace-a']);
 const received:unknown[]=[];
 nav.ready('workspace-b',target=>received.push(target));
 assert.deepEqual(received,[]);
 nav.ready('workspace-a',target=>received.push(target));
 assert.deepEqual(received,[source]);
 nav.ready('workspace-a',target=>received.push(target));
 assert.deepEqual(received,[source]);
});

test('cancelled or denied workspace access never opens its source', async()=>{
 const nav=createMenuNavigation({openWorkspace:async()=>false});
 await nav.request(source);
 const received:unknown[]=[];
 nav.ready('workspace-a',target=>received.push(target));
 assert.deepEqual(received,[]);
});

test('failed save prevents navigation and keeps the error observable', async()=>{
 const nav=createMenuNavigation({openWorkspace:async()=>{throw Error('Не удалось сохранить изменения')}});
 await assert.rejects(nav.request(source),/Не удалось сохранить изменения/);
 const received:unknown[]=[];
 nav.ready('workspace-a',target=>received.push(target));
 assert.deepEqual(received,[]);
});

test('a newer request replaces an older pending source', async()=>{
 const nav=createMenuNavigation({openWorkspace:async()=>true});
 await nav.request(source);
 const newer={...source,workspaceId:'workspace-b',id:'note-b'};
 await nav.request(newer);
 const received:unknown[]=[];
 nav.ready('workspace-a',target=>received.push(target));
 nav.ready('workspace-b',target=>received.push(target));
 assert.deepEqual(received,[newer]);
});

test('repeated requests work when the workspace is already open', async()=>{
 const nav=createMenuNavigation({openWorkspace:async()=>true});
 const received:unknown[]=[];
 await nav.request(source);
 nav.ready('workspace-a',target=>received.push(target));
 const second={...source,id:'note-b'};
 await nav.request(second);
 nav.ready('workspace-a',target=>received.push(target));
 assert.deepEqual(received,[source,second]);
});

test('an older slow open cannot replace a newer request', async()=>{
 let resolveOld!:(opened:boolean)=>void;
 const nav=createMenuNavigation({openWorkspace:id=>id==='workspace-a'?new Promise(resolve=>{resolveOld=resolve}):Promise.resolve(true)});
 const old=nav.request(source);
 const newer={...source,workspaceId:'workspace-b',id:'note-b'};
 await nav.request(newer);
 resolveOld(true);
 await old;
 const received:unknown[]=[];
 nav.ready('workspace-a',target=>received.push(target));
 nav.ready('workspace-b',target=>received.push(target));
 assert.deepEqual(received,[newer]);
});

test('a board that becomes ready before open completes still receives its source', async()=>{
 let resolveOpen!:(opened:boolean)=>void;
 const nav=createMenuNavigation({openWorkspace:()=>new Promise(resolve=>{resolveOpen=resolve})});
 const pending=nav.request(source);
 const received:unknown[]=[];
 nav.ready('workspace-a',target=>received.push(target));
 assert.deepEqual(received,[]);
 resolveOpen(true);
 await pending;
 assert.deepEqual(received,[source]);
});
