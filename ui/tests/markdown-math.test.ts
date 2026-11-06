import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import ReactMarkdown from 'react-markdown';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import {remarkCurrency} from '../src/remarkCurrency.ts';
import {markMathSearch} from '../src/mathSearchMark.ts';

function render(text:string){return renderToStaticMarkup(React.createElement(ReactMarkdown,{remarkPlugins:[remarkMath,remarkCurrency],rehypePlugins:[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]],children:text}));}
test('renders inline and display math',()=>{const html=render('Значение $x^2$\n\n$$\ny=2x\n$$');assert.match(html,/class="katex"/);assert.match(html,/class="katex-display"/);});
test('leaves literal math syntax inside code',()=>{const html=render('`$x$`\n\n```python\nvalue = "$x$"\n```');assert.doesNotMatch(html,/class="katex/);assert.match(html,/\$x\$/);});
test('does not allow KaTeX html or links via untrusted commands',()=>{const html=render('$\\href{https://example.org}{open}$');assert.doesNotMatch(html,/<a\s/);});
test('preserves currency pairs without hiding neighbouring math',()=>{const html=render('Цена $5 и $10; формула $x^2$.');assert.match(html,/Цена \$5 и \$10/);assert.equal((html.match(/class="katex"/g)||[]).length,1);});
test('preserves numeric math and ordinary currency in code',()=>{const html=render('$5^2$ и `$5 и $10`');assert.equal((html.match(/class="katex"/g)||[]).length,1);assert.match(html,/\$5 и \$10/);});
test('preserves signed and formatted prices',()=>{assert.match(render('От $-5 до $10'),/\$-5 до \$10/);assert.match(render('От $5 до $**10**'),/\$5 до \$<strong>10<\/strong>/);});
test('marks a matched formula while keeping its rendered math',()=>{
 const plugin=()=> (tree:any)=>{const visit=(n:any)=>{if(!n.children)return;n.children=n.children.map((child:any)=>markMathSearch(child,4,5)??(visit(child),child));};visit(tree);};
 const html=renderToStaticMarkup(React.createElement(ReactMarkdown,{remarkPlugins:[remarkMath,remarkCurrency,plugin],rehypePlugins:[rehypeKatex],children:'abc $x^2$ def'}));
 assert.match(html,/<mark><span class="katex"/);
});
