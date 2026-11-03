import {test} from 'node:test';
import assert from 'node:assert/strict';
import {locateNoteChunk} from '../src/noteSearchLocation.ts';
test('relocates a unique chunk after unsaved Unicode edits without corrupting offsets',()=>{
 const chunk={text:'beta',start:6,end:10};
 assert.deepEqual(locateNoteChunk('alpha beta gamma',chunk),chunk);
 assert.deepEqual(locateNoteChunk('😀 prefix alpha beta gamma',chunk),{...chunk,start:15,end:19});
 assert.equal(locateNoteChunk('beta beta',chunk),null);
 assert.equal(locateNoteChunk('changed',chunk),null);
});
test('keeps valid offsets for a truncated source excerpt and an unspecified range',()=>{
 const excerpt={text:'beta',start:6,end:16};
 assert.equal(locateNoteChunk('alpha beta gamma',excerpt),excerpt);
 const source={text:'',start:null,end:null};
 assert.equal(locateNoteChunk('text',source),source);
});
