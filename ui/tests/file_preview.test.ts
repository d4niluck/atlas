import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { decodePlainText } from '../src/filePreviewText.ts';

test('показывает UTF-8 файлы без изменения данных', () => {
  assert.equal(decodePlainText(new TextEncoder().encode('<root>Привет</root>')), '<root>Привет</root>');
  assert.equal(decodePlainText(new TextEncoder().encode('print("hello")')), 'print("hello")');
  assert.equal(decodePlainText(new TextEncoder().encode('{"a": 1}')), '{"a": 1}');
});

test('отклоняет бинарные и повреждённые данные', () => {
  assert.equal(decodePlainText(new Uint8Array([0, 1, 2])), null);
  assert.equal(decodePlainText(new Uint8Array([0xff, 0xfe, 0x10])), null);
});
