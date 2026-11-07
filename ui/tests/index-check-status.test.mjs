import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

test('проверка индекса показывается отдельно с возможностью остановки',async()=>{
 const code=await readFile('src/main.tsx','utf8');
 const start=code.indexOf('<div className="processing" role="status">');
 const end=code.indexOf('<div className="workspace">',start);
 assert.ok(start>=0&&end>start);
 const status=code.slice(start-90,end);
 assert.match(status,/semantic\.checking/,'проверка должна показываться в статусе');
 assert.match(status,/Проверка индекса/,'проверка не должна называться индексацией');
 assert.match(status,/semantic\.checking[^\n]{0,200}<button/,'проверку можно остановить');
 assert.match(status,/\/semantic\/cancel/,'отмена использует существующий обработчик');
});
