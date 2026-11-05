import {test} from 'node:test';
import assert from 'node:assert/strict';
import {nextNoteMode} from '../src/note-mode.ts';
test('полный экран позволяет читать и редактировать с двумя панелями',()=>{
 const full=nextNoteMode({expanded:false,editing:false},'expand');
 assert.deepEqual(full,{expanded:true,editing:true});
 assert.deepEqual(nextNoteMode(full,'toggle-edit'),{expanded:true,editing:false});
 assert.deepEqual(nextNoteMode({expanded:true,editing:false},'toggle-edit'),full);
 assert.deepEqual(nextNoteMode(full,'expand'),{expanded:false,editing:true});
});
