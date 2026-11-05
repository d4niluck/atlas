import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { citedSources } from '../src/citedSources.ts';
test('keeps original citation numbers, deduplicates and ignores invalid references', () => {
 const sources = ['one','two','three'];
 assert.deepEqual(citedSources('Только [2], повтор [2], и [9]', sources), [{number:2,source:'two'}]);
 assert.deepEqual(citedSources('Без ссылок', sources), []);
 assert.deepEqual(citedSources('[3] и [1]', sources), [{number:1,source:'one'},{number:3,source:'three'}]);
});
