import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {parseFragment} from 'parse5';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

function walk(node,predicate){if(predicate(node))return node;for(const child of node.childNodes??[]){const found=walk(child,predicate);if(found)return found}}
function attr(node,name){return node.attrs?.find(item=>item.name===name)?.value}

test('results do not repeat the workspace and query underneath the search controls',async()=>{
 const source=await readFile('src/MenuBarSearch.tsx','utf8');
 assert.doesNotMatch(source, /className=["']menu-search-context["']/,'не показывать отдельную подпись пространства и запроса');
});

test('semantic-search control has a plus symbol and accessible name',async()=>{
 const dir=await mkdtemp(join(process.cwd(),'.atlas-tidy-test-'));
 try {
  const out=join(dir,'component.mjs');
  await build({entryPoints:['src/MenuBarSearch.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd(),external:['react','react-dom'],loader:{'.css':'empty'}});
  const {MenuBarSearch}=await import(pathToFileURL(out).href);
  const root=parseFragment(renderToStaticMarkup(React.createElement(MenuBarSearch,{onNavigate:()=>{},onOpenSettings:()=>{}})));
  const button=walk(root,n=>n.tagName==='button'&&attr(n,'aria-label')==='По смыслу');
  assert.ok(button,'кнопка имеет доступное название');
  const svg=walk(button,n=>n.tagName==='svg');
  const plus=walk(button,n=>n.nodeName==='#text'&&/[+＋]/.test(n.value));
  assert.ok(svg||plus,'иконка — лупа с плюсом или знак плюс');
 } finally {await rm(dir,{recursive:true,force:true})}
});
