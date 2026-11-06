import {test} from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {renderToStaticMarkup} from 'react-dom/server';
import React from 'react';
import {parseFragment} from 'parse5';
import {mkdtemp,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

function walk(node, predicate) {
 if (predicate(node)) return node;
 for (const child of node.childNodes ?? []) { const found=walk(child,predicate); if(found)return found; }
}
function attr(node,name){return node.attrs?.find(item=>item.name===name)?.value}
async function render(){
 const dir=await mkdtemp(join(process.cwd(),'.atlas-layout-test-'));
 try {
  const out=join(dir,'component.mjs');
  await build({entryPoints:['src/MenuBarSearch.tsx'],outfile:out,bundle:true,platform:'node',format:'esm',absWorkingDir:process.cwd(),external:['react','react-dom'],loader:{'.css':'empty'}});
  const {MenuBarSearch}=await import(pathToFileURL(out).href);
  return parseFragment(renderToStaticMarkup(React.createElement(MenuBarSearch,{onNavigate:()=>{},onOpenSettings:()=>{}})),{sourceCodeLocationInfo:true});
 } finally {await rm(dir,{recursive:true,force:true})}
}

test('search mode controls share the search row and precede the input',async()=>{
 const root=await render();
 const row=walk(root,n=>attr(n,'class')?.split(' ').includes('menu-search-query-row'));
 assert.ok(row,'элементы поиска объединены в строку');
 const group=walk(row,n=>attr(n,'aria-label')==='Режим поиска');
 const input=walk(row,n=>attr(n,'aria-label')==='Поисковый запрос');
 assert.ok(group&&input,'строка содержит переключатели и поле');
 assert.ok(group.sourceCodeLocation.startOffset<input.sourceCodeLocation.startOffset,'переключатели расположены перед полем');
});

test('workspace selector and settings control share the header',async()=>{
 const root=await render();
 const select=walk(root,n=>n.tagName==='select'&&attr(n,'aria-label')==='Пространство');
 const settings=walk(root,n=>attr(n,'aria-label')==='Настройки поиска');
 assert.ok(select&&settings);
 let head=select.parentNode;while(head&&head.tagName!=='header')head=head.parentNode;
 assert.ok(head&&walk(head,n=>n===settings));
});
