import {test} from 'node:test';
import assert from 'node:assert/strict';
import {selectSearchMode} from '../src/searchMode.ts';
test('two search modes can be toggled without turning both off',()=>{
 assert.equal(selectSearchMode('lexical','lexical','semantic'),'hybrid');
 assert.equal(selectSearchMode('hybrid','hybrid','lexical'),'semantic');
 assert.equal(selectSearchMode('hybrid','hybrid','semantic'),'lexical');
 assert.equal(selectSearchMode('semantic','semantic','semantic'),'semantic');
 assert.equal(selectSearchMode('lexical','lexical','lexical'),'lexical');
});
test('returning from RAG restores both modes and permits choosing a single one',()=>{
 assert.equal(selectSearchMode('rag','hybrid','lexical'),'hybrid');
 assert.equal(selectSearchMode('rag','hybrid','semantic'),'hybrid');
 assert.equal(selectSearchMode('rag','lexical','semantic'),'semantic');
});
