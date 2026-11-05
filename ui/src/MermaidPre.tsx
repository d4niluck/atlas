import React, {isValidElement, useEffect, useState} from 'react';
import DOMPurify from 'dompurify';
import {prepareMermaid} from './mermaid-safety';
import {applyMermaidFontSizes} from './mermaid-svg';
import './mermaid.css';

let nextId = 0;
let initialized = false;

function MermaidDiagram({source}: {source:string}) {
 const [result,setResult] = useState<{html?:string; error?:string}>({});
 useEffect(() => {
  let cancelled = false;
  const prepared = prepareMermaid(source);
  if (prepared.error) { setResult({error:prepared.error}); return; }
  setResult({});
  void (async () => {
   try {
    const mermaid = (await import('mermaid')).default;
    if (!initialized) {
     mermaid.initialize({startOnLoad:false, securityLevel:'strict', suppressErrorRendering:true,
      maxTextSize:12000, maxEdges:200, htmlLabels:false, flowchart:{htmlLabels:false}, theme:'neutral'});
     initialized = true;
    }
    const container = document.createElement('div');
    container.style.cssText='position:fixed;left:-100000px;top:0;visibility:hidden';
    document.body.appendChild(container);
    try {
     const {svg} = await mermaid.render(`atlas-mermaid-${++nextId}`, prepared.source, container);
     const safe = DOMPurify.sanitize(svg, {USE_PROFILES:{svg:true,svgFilters:true}, FORBID_TAGS:['script','foreignObject','a'], FORBID_ATTR:['href','xlink:href']});
     const clean = applyMermaidFontSizes(safe, prepared.fontSizes);
     const html = `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'"><style>html,body{margin:0;background:${prepared.background}}svg{display:block;max-width:100%;height:auto;margin:auto}</style></head><body>${clean}</body></html>`;
     if (!cancelled) setResult({html});
    } finally { container.remove(); }
   } catch {
    if (!cancelled) setResult({error:'Проверьте синтаксис Mermaid'});
   }
  })();
  return () => { cancelled=true; };
 }, [source]);
 if (result.error) return <div className="mermaid-error"><div role="alert">Не удалось построить диаграмму: {result.error}</div><pre><code>{source}</code></pre></div>;
 if (!result.html) return <div role="status">Строим диаграмму…</div>;
 return <iframe className="mermaid-frame" title="Диаграмма Mermaid" sandbox="" srcDoc={result.html}/>;
}

/** Only fenced Mermaid blocks are rendered; all other Markdown code stays unchanged. */
export function MarkdownPre({children, node: _node, ...props}:React.HTMLAttributes<HTMLPreElement> & {node?:unknown}) {
 if (isValidElement<{className?:string;children?:React.ReactNode; 'data-atlas-search-range'?:string}>(children)) {
  const range=children.props['data-atlas-search-range']?.match(/^(\d+):(\d+)$/);
  if(range && typeof children.props.children==='string') {
   const source=children.props.children;const start=Number(range[1]),end=Number(range[2]);
   return <pre {...props}><code className={children.props.className}>{source.slice(0,start)}<mark>{source.slice(start,end)}</mark>{source.slice(end)}</code></pre>;
  }
 }
 if (isValidElement<{className?:string;children?:React.ReactNode}>(children) && /(?:^|\s)language-mermaid(?:\s|$)/i.test(children.props.className || '')) {
  return <MermaidDiagram source={String(children.props.children || '').replace(/\n$/, '')}/>;
 }
 return <pre {...props}>{children}</pre>;
}
