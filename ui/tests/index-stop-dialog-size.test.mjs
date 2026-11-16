import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
const require=createRequire(new URL('../package.json',import.meta.url));
const postcss=require('postcss');

test('index stop dialog overrides editor size in the CSS cascade',()=>{
  const main=readFileSync(new URL('../src/main.tsx',import.meta.url),'utf8');
  const files=[...main.matchAll(/^import ['"](\.\/[^'"]+\.css)['"];?/gm)].map(m=>m[1]);
  const winners={};
  for(const file of files){
    const css=postcss.parse(readFileSync(new URL('../src/'+file.slice(2),import.meta.url),'utf8'));
    css.walkRules(rule=>{
      if(rule.parent.type!=='root')return;
      const selector=rule.selector.trim();
      if(!['.editor','.index-stop-dialog','.editor.index-stop-dialog'].includes(selector))return;
      const specificity=(selector.match(/\./g)||[]).length;
      rule.walkDecls(/^(width|height)$/,decl=>{
        const old=winners[decl.prop];
        if(!old||specificity>=old.specificity)winners[decl.prop]={specificity,value:decl.value};
      });
    });
  }
  assert.equal(winners.height?.value,'auto','editor height must not override compact dialog');
  assert.match(winners.width?.value||'',/420px/,'dialog must use compact width');
});
