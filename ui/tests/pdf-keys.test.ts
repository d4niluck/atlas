import {test} from 'node:test';
import assert from 'node:assert/strict';
import {pdfPageDirection} from '../src/pdfKeys.ts';

test('PDF arrow keys change pages unless the user is editing or using a shortcut', () => {
  const event = {key:'ArrowLeft', altKey:false, ctrlKey:false, metaKey:false, shiftKey:false, defaultPrevented:false, target:null};
  assert.equal(pdfPageDirection(event), -1);
  assert.equal(pdfPageDirection({...event, key:'ArrowRight'}), 1);
  assert.equal(pdfPageDirection({...event, metaKey:true}), 0);
  assert.equal(pdfPageDirection({...event, defaultPrevented:true}), 0);
  assert.equal(pdfPageDirection({...event, target:{closest:()=>({})} as unknown as EventTarget}), 0);
  assert.deepEqual([true, false].map(active => pdfPageDirection({...event, key:'ArrowRight'}, active)), [1, 0]);
});
