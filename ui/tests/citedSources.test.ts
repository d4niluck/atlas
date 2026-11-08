import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { citedSources } from '../src/citedSources.ts';
test('keeps original citation numbers, deduplicates and ignores invalid references', () => {
 const sources = ['one','two','three'];
 assert.deepEqual(citedSources('Только [2], повтор [2], и [9]', sources), [{number:2,source:'two'}]);
 assert.deepEqual(citedSources('Без ссылок', sources), []);
 assert.deepEqual(citedSources('[3] и [1]', sources), [{number:1,source:'one'},{number:3,source:'three'}]);
 assert.deepEqual(citedSources('Факты [1][2].', ['one','three']), [{number:1,source:'one'},{number:2,source:'three'}]);
});
test('ignores numbers in links, code and formulas, and detects sources after a code fence', () => {
 const text = '[2](https://example.com) [2][document] [2][] `a=[2]` $x_{[2]}$\n```py\nprint("```")\na=[2]\n````\nИсточник [1]';
 assert.deepEqual(citedSources(text, ['one','two']), [{number:1,source:'one'}]);
});
test('prices do not hide subsequent citations', () => {
 for (const text of ['Цена $20. Подтверждение [2].', 'Цена $5 и $10. Подтверждение [2].']) assert.deepEqual(citedSources(text, ['one','two']), [{number:2,source:'two'}]);
});
