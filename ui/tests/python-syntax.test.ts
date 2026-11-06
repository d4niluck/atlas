import test from 'node:test';
import assert from 'node:assert/strict';
import {highlightPython} from '../src/python-syntax.ts';

test('Python code highlights keywords, strings and comments without accepting raw markup', async()=>{
 const result=await highlightPython('def hello():\n    return "<script>"  # comment');
 assert.ok(result);
 assert.match(result,/hljs-keyword/);
 assert.match(result,/hljs-string/);
 assert.match(result,/hljs-comment/);
 assert.ok(!result.includes('<script>'));
});

test('very large code blocks remain readable without synchronous highlighting', async()=>{
 assert.equal(await highlightPython('x'.repeat(200_001)),null);
});
