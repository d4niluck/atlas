import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile, mkdtemp, rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {build} from 'esbuild';
import React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';

// Inject an already streamed answer into the real menu component without starting a server.
async function renderAnswer(answer) {
 const dir=await mkdtemp(join(process.cwd(),'.atlas-math-test-'));
 try {
  const out=join(dir,'menu.mjs');
  const plugin={name:'answer-fixture',setup(builder){
   builder.onLoad({filter:/MenuBarSearch\.tsx$/},async args=>{
    let source=await readFile(args.path,'utf8');
    source=source.replace("import React, {useEffect, useRef, useState} from 'react';", "import React, {useEffect, useRef, useState as originalUseState} from 'react';\nlet hookIndex = 0; export function resetTestHooks() { hookIndex = 0; }\nfunction useState(initial) { const i = hookIndex++; if (i === 6) initial = globalThis.__atlasTestAnswer; if (i === 8) initial = {workspaceId:'w', workspaceName:'w', protected:false, query:'q', mode:'rag', agentic:false}; return originalUseState(initial); }");
    return {contents:source,loader:'tsx'};
   });
  }};
  await build({entryPoints:['src/MenuBarSearch.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd(),external:['react','react-dom'],loader:{'.css':'empty'},plugins:[plugin]});
  const {MenuBarSearch,resetTestHooks}=await import(pathToFileURL(out).href);
  globalThis.__atlasTestAnswer=answer;resetTestHooks();
  return renderToStaticMarkup(React.createElement(MenuBarSearch,{onNavigate:()=>{},onOpenSettings:()=>{}}));
 } finally {delete globalThis.__atlasTestAnswer;await rm(dir,{recursive:true,force:true});}
}

test('menu answer renders inline and block formulas while retaining Markdown',async()=>{
 const html=await renderAnswer('**Формула:** $x^2$\n\n$$\ny = x + 1\n$$\n\nОбычный текст');
 assert.match(html, /<strong>Формула:<\/strong>/);
 assert.match(html, /class="katex"/, 'inline formula should render using KaTeX');
 assert.match(html, /class="katex-display"/, 'block formula should render using KaTeX');
 assert.match(html, /Обычный текст/);
});

test('menu output includes formula stylesheet',async()=>{
 const sources=await Promise.all(['src/MenuBarSearch.tsx','src/main.tsx'].map(path=>readFile(path,'utf8')));
 assert.ok(sources.some(source=>/katex\/dist\/katex(?:\.min)?\.css/.test(source)),'KaTeX stylesheet is included by the application');
});

test('ordinary prices and line breaks retain the same readable form as Atlas',async()=>{
 const html=await renderAnswer('Цена $5 и $10\nВторая строка');
 assert.match(html, /\$5 и \$10/, 'prices should not be interpreted as a formula');
 assert.match(html, /<br\s*\/>/, 'a single line break remains visible');
 assert.match(html, /Вторая строка/);
});

test('code and incomplete streaming formula remain readable',async()=>{
 const html=await renderAnswer('Код: `$x$`\n\nНезавершённая формула $y =');
 assert.match(html, /<code>\$x\$<\/code>/);
 assert.match(html, /Незавершённая формула \$y =/);
});

test('untrusted formula links are not made clickable',async()=>{
 const html=await renderAnswer('$\\href{https://example.com}{link}$');
 assert.doesNotMatch(html, /<a[^>]+href="https:\/\/example\.com"/);
});
