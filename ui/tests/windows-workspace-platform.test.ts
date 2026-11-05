import {strict as assert} from 'node:assert';
import {isWindows} from '../src/workspacePlatform.ts';
assert.equal(isWindows('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/130'), true);
assert.equal(isWindows('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15'), false);
assert.equal(isWindows('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36'), false);
console.log('workspace platform: OK');
