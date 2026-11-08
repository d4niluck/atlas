import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

async function withSession(run){
 const dir=await mkdtemp(join(process.cwd(),'.atlas-voice-test-'));
 try{
  const out=join(dir,'session.mjs');
  await build({entryPoints:['src/voiceSession.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});
  const {createVoiceSession}=await import(pathToFileURL(out).href);
  await run(createVoiceSession);
 }finally{await rm(dir,{recursive:true,force:true})}
}
function fixture(){
 const calls=[];
 const stream={getTracks:()=>[{stop:()=>calls.push('track-stop')}]};
 const wav=new Uint8Array([1,2,3]);
 const dependencies={
  getUserMedia:async()=>{calls.push('microphone');return stream},
  capture:async received=>{assert.equal(received,stream);calls.push('capture');return {stop:async()=>{calls.push('capture-stop');return wav},cancel:()=>calls.push('capture-cancel')}},
  transcribe:async received=>{assert.equal(received,wav);calls.push('transcribe');return 'найденный текст'},
  cancel:async()=>{calls.push('cancel-transcription')},
 };
 return {calls,dependencies,wav};
}

test('микрофон не запрашивается до явного начала записи',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();const session=create(dependencies);
 assert.deepEqual(calls,[]);
 await session.start();assert.deepEqual(calls,['microphone','capture']);
 await session.cancel();
}));

test('модель готовится во время записи и используется для той же записи',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();let preparedId;
 dependencies.prepare=async id=>{preparedId=id;calls.push('prepare')};
 dependencies.transcribe=async(_wav,id)=>{assert.equal(id,preparedId);calls.push('transcribe');return 'текст'};
 const session=create(dependencies);await session.start();assert.ok(calls.includes('prepare'));assert.ok(!calls.includes('transcribe'));
 assert.equal(await session.stop(),'текст');assert.equal(calls.filter(x=>x==='prepare').length,1);
}));

test('отмена во время подготовки выгружает именно готовящийся процесс',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();let preparedId,finish;
 dependencies.prepare=id=>{preparedId=id;return new Promise(resolve=>{finish=resolve})};
 dependencies.cancel=async id=>{assert.equal(id,preparedId);calls.push('cancel-preparing')};
 const session=create(dependencies);await session.start();await session.cancel();assert.ok(calls.includes('cancel-preparing'));
 finish();await Promise.resolve();assert.ok(!calls.includes('transcribe'));
}));

test('по окончании записи освобождаются дорожки, возвращается распознанный текст',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();const session=create(dependencies);
 await session.start();const text=await session.stop();
 assert.equal(text,'найденный текст');
 assert.ok(calls.includes('track-stop'),'микрофон должен быть освобождён');
 assert.ok(calls.includes('transcribe'),'запись должна быть распознана');
}));

test('отмена во время записи освобождает микрофон и не запускает распознавание',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();const session=create(dependencies);
 await session.start();await session.cancel();
 assert.ok(calls.includes('track-stop'));
 assert.ok(!calls.includes('transcribe'));
}));

test('поздний ответ отменённого распознавания нельзя вставить в поле поиска',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();let resolve,notify;const entered=new Promise(r=>{notify=r});
 dependencies.transcribe=()=>new Promise(r=>{calls.push('transcribe');resolve=r;notify()});
 const session=create(dependencies);await session.start();const pending=session.stop();
 await Promise.race([entered,pending.then(()=>{throw Error('Распознавание не запущено')})]);
 await session.cancel();resolve('устаревший текст');
 assert.equal(await pending,null);
 assert.ok(calls.includes('cancel-transcription'));
}));

test('отмена во время системного запроса разрешения освобождает поздно полученный микрофон',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();let grant;
 dependencies.getUserMedia=()=>new Promise(resolve=>{grant=resolve});
 const session=create(dependencies);const starting=session.start();
 await session.cancel();grant({getTracks:()=>[{stop:()=>calls.push('track-stop')}]});
 await starting;
 assert.ok(calls.includes('track-stop'));
 assert.ok(!calls.includes('capture'));
 assert.ok(!calls.includes('transcribe'));
}));

test('отмена ожидания разрешения сразу освобождает защиту окна',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();let grant;
 dependencies.getUserMedia=()=>new Promise(resolve=>{grant=resolve});
 dependencies.cancelPermission=async()=>calls.push('release-permission');
 const session=create(dependencies);const starting=session.start();
 await session.cancel();assert.ok(calls.includes('release-permission'));
 grant({getTracks:()=>[{stop:()=>calls.push('track-stop')}]});await starting;
 assert.ok(calls.includes('track-stop'));assert.ok(!calls.includes('capture'));
}));

test('ошибка подготовки записи освобождает микрофон',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();dependencies.capture=async()=>{throw new Error('capture failed')};
 const session=create(dependencies);await assert.rejects(session.start(),/capture failed/);
 assert.ok(calls.includes('track-stop'));
}));

test('ошибка распознавания не удерживает микрофон',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();dependencies.transcribe=async()=>{throw new Error('recognition failed')};
 const session=create(dependencies);await session.start();await assert.rejects(session.stop(),/recognition failed/);
 assert.ok(calls.includes('track-stop'));
}));

test('закрытие уже завершённой записи не отменяет чужое распознавание',async()=>withSession(async create=>{
 const {calls,dependencies}=fixture();const session=create(dependencies);
 await session.start();await session.stop();await session.cancel();
 assert.ok(!calls.includes('cancel-transcription'));
}));

test('распознанный текст добавляется к текущему запросу и не удаляет ввод пользователя',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-voice-text-test-'));
 try {const out=join(dir,'text.mjs');await build({entryPoints:['src/voiceText.ts'],outfile:out,bundle:true,platform:'node',format:'esm',logLevel:'silent'});const {appendVoiceText}=await import(pathToFileURL(out).href);
 assert.equal(appendVoiceText('','привет'),'привет');
 assert.equal(appendVoiceText('машинное','обучение'),'машинное обучение');
 assert.equal(appendVoiceText('машинное ','обучение'),'машинное обучение');
 assert.equal(appendVoiceText('строка\n','продолжение'),'строка\nпродолжение');
 }finally{await rm(dir,{recursive:true,force:true})}
});
