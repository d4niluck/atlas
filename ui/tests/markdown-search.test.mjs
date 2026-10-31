import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp, rm} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {pathToFileURL} from 'node:url';

test('Markdown preserves formatting and highlights Unicode source offsets', async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-md-'));
 try {
  const file=join(dir,'component.mjs');
  await build({entryPoints:['src/MarkdownSearchExcerpt.tsx'],outfile:file,bundle:true,platform:'node',format:'esm',external:['react','react/jsx-runtime']});
  const {MarkdownSearchExcerpt}=await import(pathToFileURL(file).href);
  const text='# Заголовок\n\n😀 **найдено** здесь';
  const start=Array.from(text.slice(0,text.indexOf('найдено'))).length;
  const html=renderToStaticMarkup(React.createElement(MarkdownSearchExcerpt,{text,start,end:start+7}));
  assert.match(html,/<h1>Заголовок<\/h1>/);
  assert.match(html,/<strong><mark>найдено<\/mark><\/strong>/);
  assert.doesNotMatch(html,/<pre/);
 } finally {await rm(dir,{recursive:true,force:true})}
});
