import {strict as assert} from 'node:assert';
import {readFile} from 'node:fs/promises';
import {test} from 'node:test';

test('PDF search and page configuration work without newer Map APIs', async () => {
  const prototype = Map.prototype as Map<unknown, unknown> & {getOrInsertComputed?: unknown};
  const original = prototype.getOrInsertComputed;
  delete prototype.getOrInsertComputed;
  try {
    const {getDocument} = await import('../src/pdfCompat.ts');
    const bytes = await readFile(new URL('./fixtures/two-pages.pdf', import.meta.url));
    const task = getDocument({data: new Uint8Array(bytes)});
    try {
      const pdf = await task.promise;
      assert.equal(pdf.numPages, 2);
      assert.equal((await pdf.getPage(1)).pageNumber, 1);
      assert.ok(await pdf.getOptionalContentConfig());
    } finally { await task.destroy(); }
  } finally {
    if (original === undefined) delete prototype.getOrInsertComputed;
    else prototype.getOrInsertComputed = original;
  }
});
