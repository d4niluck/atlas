import assert from 'node:assert/strict';
import {test} from 'node:test';
import {applyFormat} from '../src/markdown-format.ts';

test('оборачивает выделенный текст и сохраняет выделение', () => {
 const r=applyFormat('текст пример',0,5,'bold');
 assert.deepEqual(r,{text:'**текст** пример',start:2,end:7});
});
test('добавляет список к каждой выделенной строке', () => {
 const r=applyFormat('один\nдва\nтри',1,7,'list');
 assert.equal(r.text,'- один\n- два\nтри');
});
test('вставляет ссылку с выделенным названием', () => {
 const r=applyFormat('источник',0,8,'link');
 assert.equal(r.text,'[источник](https://)');
 assert.equal(r.text.slice(r.start,r.end),'https://');
});

test('обрабатывает пустую первую строку в начале документа', () => {
 assert.equal(applyFormat('\nдва',0,0,'quote').text, '> \nдва');
});
