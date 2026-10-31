import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import {historyAgentic} from '../src/historyAgentic.ts';
test('restores agentic flag and partial-result warning', () => {
  assert.deepEqual(historyAgentic('rag', true, 'Часть поиска недоступна'), {agentic:true,error:'Часть поиска недоступна'});
  assert.deepEqual(historyAgentic('rag', false, null), {agentic:false,error:''});
  assert.deepEqual(historyAgentic('rag', null, null), {agentic:false,error:'Режим исходного запроса неизвестен.'});
  assert.deepEqual(historyAgentic('semantic', null, null), {agentic:false,error:''});
});
