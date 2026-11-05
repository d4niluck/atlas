import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {mkdtemp, rm, writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

test('highlights inline code and source ranges hidden by Markdown syntax',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-md-loc-'));
 try{
  const entry=join(dir,'entry.ts');const output=join(dir,'out.mjs');
  await writeFile(entry,`export {MarkdownSearchExcerpt} from '../src/MarkdownSearchExcerpt'; export {MarkdownPre} from '../src/MermaidPre';`);
  await build({entryPoints:[entry],outfile:output,bundle:true,platform:'node',format:'esm',external:['react','react/jsx-runtime','mermaid','dompurify']});
  const {MarkdownSearchExcerpt,MarkdownPre}=await import(pathToFileURL(output).href);
  const cases=[['A `needle` B','needle',/<code><mark>needle<\/mark><\/code>/],['![secret](image.png)','secret',/Фрагмент Markdown: <mark>secret<\/mark>/],['X &amp; Y','&amp;',/Фрагмент Markdown: <mark>&amp;amp;<\/mark>/],['```mermaid\r\ngraph TD\r\n A-->B\r\n```','A-->B',/<mark>A--&gt;B<\/mark>/],['```js\nconst hello=1\n```','hello',/<mark>hello<\/mark>/]];
  for(const [text,selected,expected] of cases){const start=Array.from(text.slice(0,text.indexOf(selected))).length;const html=renderToStaticMarkup(React.createElement(MarkdownSearchExcerpt,{text,start,end:start+selected.length,components:{pre:MarkdownPre}}));assert.match(html,expected)}
 } finally {await rm(dir,{recursive:true,force:true})}
});
