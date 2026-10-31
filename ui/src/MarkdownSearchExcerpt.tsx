import React, {useEffect, useMemo, useRef} from 'react';
import ReactMarkdown, {type Components} from 'react-markdown';
import remarkGfm from 'remark-gfm';

// Source offsets are code points; Markdown parser positions use UTF-16 offsets.
export function MarkdownSearchExcerpt({text, start, end, components}: {text:string;start:number;end:number;components?:Components}) {
  const container = useRef<HTMLDivElement>(null);
  const plugin = useMemo(() => {
    const chars=Array.from(text);
    const from=chars.slice(0,Math.max(0,start)).join('').length;
    const to=chars.slice(0,Math.max(start,end)).join('').length;
    return () => (tree:any) => {
      function visit(node:any) {
        if (!Array.isArray(node.children)) return;
        node.children=node.children.flatMap((child:any) => {
          if(child.type!=='text') {visit(child);return [child]}
          const a=child.position?.start?.offset,b=child.position?.end?.offset;
          // Escapes/entities can change rendered text; do not guess their positions.
          if(typeof a!=='number'||typeof b!=='number'||text.slice(a,b)!==child.value||to<=a||from>=b)return [child];
          const left=Math.max(0,from-a),right=Math.min(child.value.length,to-a);
          return [child.value.slice(0,left) && {type:'text',value:child.value.slice(0,left)},
            {type:'strong',data:{hName:'mark'},children:[{type:'text',value:child.value.slice(left,right)}]},
            child.value.slice(right) && {type:'text',value:child.value.slice(right)}].filter(Boolean);
        });
      }
      visit(tree);
    };
  },[text,start,end]);
  useEffect(()=>{container.current?.querySelector('mark')?.scrollIntoView({block:'center'})},[text,start,end]);
  return <div ref={container}><ReactMarkdown components={components} remarkPlugins={[remarkGfm,plugin]}>{text}</ReactMarkdown></div>;
}
