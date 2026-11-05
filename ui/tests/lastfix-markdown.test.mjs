import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('single line break renders and search offsets still highlight second line',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-md-'));
 try {
  const file=join(dir,'component.mjs');
  await build({entryPoints:['src/MarkdownSearchExcerpt.tsx'],outfile:file,bundle:true,platform:'node',format:'esm',external:['react','react/jsx-runtime']});
  const {MarkdownSearchExcerpt}=await import(pathToFileURL(file).href);
  const text='Первая строка\n😀 **нужный текст** далее';
  const start=Array.from(text.slice(0,text.indexOf('нужный'))).length;
  const html=renderToStaticMarkup(React.createElement(MarkdownSearchExcerpt,{text,start,end:start+12}));
  assert.match(html,/<br\s*\/?>/);
  assert.match(html,/<strong><mark>нужный текст<\/mark><\/strong>/);
 } finally {await rm(dir,{recursive:true,force:true})}
});
