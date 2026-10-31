import { strict as assert } from 'node:assert';
import { mergeDownloadStatus } from '../src/modelDownloadStatus.ts';
assert.equal(mergeDownloadStatus('error:сбой','idle'),'error:сбой');
assert.equal(mergeDownloadStatus('error:сбой','downloading:2'),'downloading:2');
assert.equal(mergeDownloadStatus('downloading:50','complete'),'complete');
