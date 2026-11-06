import React, {useEffect, useMemo, useRef} from 'react';
import ReactMarkdown, {type Components} from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import {remarkCurrency} from './remarkCurrency';
import {markMathSearch} from './mathSearchMark';
import rehypeKatex from 'rehype-katex';
import {remarkSoftBreaks} from './remarkSoftBreaks';
import './search-highlight.css';

// Source offsets are code points; Markdown parser positions use UTF-16 offsets.
export function MarkdownSearchExcerpt({text, start, end, components}: {text:string;start:number;end:number;components?:Components}) {
  const container = useRef<HTMLDivElement>(null);
  const plugin = useMemo(() => {
    const chars=Array.from(text);
    const from=chars.slice(0,Math.max(0,start)).join('').length;
    const to=chars.slice(0,Math.max(start,end)).join('').length;
    return () => (tree:any) => {
      let marked=false;
      function visit(node:any) {
        if (!Array.isArray(node.children)) return;
        node.children=node.children.flatMap((child:any) => {
          const a=child.position?.start?.offset,b=child.position?.end?.offset;
          const mathMark=markMathSearch(child,from,to);if(mathMark){marked=true;return [mathMark]}
          if(child.type==='inlineCode' && typeof a==='number' && typeof b==='number' && to>a && from<b) {
            const raw=text.slice(a,b), normalized=raw.replace(/\r\n/g,'\n');
            const sourceStart=normalized.indexOf(child.value);
            const selectedFrom=raw.slice(0,Math.max(0,from-a)).replace(/\r\n/g,'\n').length-sourceStart;
            const selectedTo=raw.slice(0,Math.max(0,to-a)).replace(/\r\n/g,'\n').length-sourceStart;
            if(sourceStart>=0 && selectedTo>0 && selectedFrom<child.value.length){
              const left=Math.max(0,selectedFrom),right=Math.min(child.value.length,selectedTo);marked=true;
              return [{type:'strong',data:{hName:'code'},children:[
                child.value.slice(0,left) && {type:'text',value:child.value.slice(0,left)},
                {type:'strong',data:{hName:'mark'},children:[{type:'text',value:child.value.slice(left,right)}]},
                child.value.slice(right) && {type:'text',value:child.value.slice(right)}].filter(Boolean)}];
            }
          }
          if(child.type==='code' && typeof a==='number' && typeof b==='number' && to>a && from<b) {
            // Code (including Mermaid) has no text children: mark its source
            // range so the preview can render that block as readable source.
            const raw=text.slice(a,b), normalized=raw.replace(/\r\n/g,'\n');
            const sourceStart=normalized.indexOf(child.value);
            const selectedFrom=raw.slice(0,Math.max(0,from-a)).replace(/\r\n/g,'\n').length-sourceStart;
            const selectedTo=raw.slice(0,Math.max(0,to-a)).replace(/\r\n/g,'\n').length-sourceStart;
            if(sourceStart>=0 && selectedTo>0 && selectedFrom<child.value.length){
              marked=true;child.data={...child.data,hProperties:{...child.data?.hProperties,'data-atlas-search-range':`${Math.max(0,selectedFrom)}:${Math.min(child.value.length,selectedTo)}`}};
            }
            return [child];
          }
          if(child.type!=='text') {visit(child);return [child]}
          // Escapes/entities can change rendered text; do not guess their positions.
          if(typeof a!=='number'||typeof b!=='number'||text.slice(a,b)!==child.value||to<=a||from>=b)return [child];
          const left=Math.max(0,from-a),right=Math.min(child.value.length,to-a);marked=true;
          return [child.value.slice(0,left) && {type:'text',value:child.value.slice(0,left)},
            {type:'strong',data:{hName:'mark'},children:[{type:'text',value:child.value.slice(left,right)}]},
            child.value.slice(right) && {type:'text',value:child.value.slice(right)}].filter(Boolean);
        });
      }
      visit(tree);
      if(!marked && Array.isArray(tree.children) && to>from){
        // Some Markdown positions refer to hidden syntax (URLs or image alt
        // text). Show the safe source excerpt beside its owning block.
        const excerpt=text.slice(from,to).slice(0,500);
        const at=tree.children.findIndex((n:any)=>typeof n.position?.end?.offset==='number'&&n.position.end.offset>from);
        tree.children.splice(at<0?tree.children.length:at,0,{type:'paragraph',children:[
          {type:'text',value:'Фрагмент Markdown: '},
          {type:'strong',data:{hName:'mark'},children:[{type:'text',value:excerpt}]}]});
      }
    };
  },[text,start,end]);
  useEffect(()=>{
    const root=container.current;const mark=root?.querySelector('mark');if(!root||!mark)return;
    let active=true;let frame=0;const viewport=root.closest('.note-content, .note-preview');
    const hadPending=Boolean(root.querySelector('[role="status"]'));
    const focus=()=>{if(active)mark.scrollIntoView({block:'center'})};
    const settle=()=>{cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{focus();if(!root.querySelector('[role="status"]')){if(hadPending){const marks=Array.from(root.querySelectorAll('mark'));for(const node of marks)node.style.animation='none';void root.offsetWidth;for(const node of marks)node.style.animation='';}stop()}})};
    const observer=new MutationObserver(settle);observer.observe(root,{childList:true,subtree:true});
    const resize=typeof ResizeObserver==='function'?new ResizeObserver(settle):null;resize?.observe(root);
    const stop=()=>{if(!active)return;active=false;observer.disconnect();resize?.disconnect();cancelAnimationFrame(frame);viewport?.removeEventListener('wheel',stop);viewport?.removeEventListener('touchstart',stop);viewport?.removeEventListener('pointerdown',stop);window.removeEventListener('keydown',onKey)};
    const onKey=(e:KeyboardEvent)=>{if(['ArrowUp','ArrowDown','PageUp','PageDown','Home','End',' '].includes(e.key))stop()};
    viewport?.addEventListener('wheel',stop,{passive:true});viewport?.addEventListener('touchstart',stop,{passive:true});viewport?.addEventListener('pointerdown',stop,{passive:true});window.addEventListener('keydown',onKey);
    focus();settle();const timeout=window.setTimeout(stop,15000);
    return()=>{window.clearTimeout(timeout);stop()};
  },[text,start,end]);
  return <div ref={container} key={`${start}:${end}`} className="search-highlight"><ReactMarkdown components={components} remarkPlugins={[remarkGfm,remarkMath,remarkCurrency,plugin,remarkSoftBreaks]} rehypePlugins={[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]]}>{text}</ReactMarkdown></div>;
}
