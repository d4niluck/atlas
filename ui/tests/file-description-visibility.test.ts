import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import {supportsFileDescription} from '../src/fileDescriptionVisibility.ts';
test('descriptions are offered for readable PDFs and remain unchanged for other files',()=>{
 assert.equal(supportsFileDescription('book.PDF','available'),true);
 assert.equal(supportsFileDescription('book.md','available'),false);
 assert.equal(supportsFileDescription('file.bin','unsupported'),true);
 assert.equal(supportsFileDescription('book.md','available','description'),true);
});
