/// <reference types="vite/client" />
import {useEffect, useRef, useState} from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import {remarkCurrency} from './remarkCurrency';
import rehypeKatex from 'rehype-katex';
import {remarkSoftBreaks} from './remarkSoftBreaks';
import {MarkdownCodePre as MarkdownPre} from './MarkdownCodePre';
import {PythonCode} from './PythonCode';
import {getDocument, GlobalWorkerOptions} from './pdfCompat';
import {locatePdfFragment, type PdfMatch} from './pdfSearch';
import type {SearchChunk} from './SearchResults';
import {Util} from 'pdfjs-dist/legacy/build/pdf.mjs';
import type {PDFDocumentProxy, RenderTask} from 'pdfjs-dist/legacy/build/pdf.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import './file-preview-layout.css';
import './search-highlight.css';

GlobalWorkerOptions.workerSrc = workerUrl;


export function PlainFilePreview({url, markdown=false, filename=''}: {url:string; markdown?:boolean; filename?:string}) {
 const [page,setPage]=useState(0),[total,setTotal]=useState<number|null>(null),[value,setValue]=useState(''),[status,setStatus]=useState('Открываем файл…');
 useEffect(()=>{const abort=new AbortController();setStatus('Открываем файл…');(async()=>{try{
 const step=65536,start=page*step,end=start+step,base=Math.max(0,start-3);
 let res=await fetch(url,{headers:{Range:`bytes=${base}-${end+2}`},signal:abort.signal});if(res.status===416&&page===0)res=await fetch(url,{signal:abort.signal});if(!res.ok||!res.body)throw Error();
 const length=res.headers.get('Content-Range')?.split('/')[1]??res.headers.get('Content-Length');if(length!==null)setTotal(Number(length));
 const reader=res.body.getReader();let pos=res.status===206?base:0;const chunks:Uint8Array[]=[];
 while(true){const part=await reader.read();if(part.done)break;const from=Math.max(0,base-pos),to=Math.min(part.value.length,end+3-pos);if(to>from)chunks.push(part.value.slice(from,to));pos+=part.value.length;if(pos>=end+3){await reader.cancel();break;}}
 const bytes=new Uint8Array(chunks.reduce((n,c)=>n+c.length,0));let cursor=0;for(const chunk of chunks){bytes.set(chunk,cursor);cursor+=chunk.length}
 let left=start-base,right=Math.min(bytes.length,end-base);while(left<bytes.length&&(bytes[left]&192)===128)left++;while(right<bytes.length&&(bytes[right]&192)===128)right++;
 const text=new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(left,right));if(!abort.signal.aborted){setValue(text);setStatus('')}
 }catch{if(!abort.signal.aborted)setStatus('Не удалось открыть файл.')}})();return()=>abort.abort()},[url,page]);
 return status?<p role="status">{status}</p>:<><div className="pdf-preview-controls"><button aria-label="Предыдущая часть" disabled={page===0} onClick={()=>setPage(page-1)}>←</button><label>Часть <input style={{width:72}} aria-label="Номер части" type="number" min="1" max={total!==null?Math.max(1,Math.ceil(total/65536)):undefined} defaultValue={page+1} key={page} onKeyDown={e=>{if(e.key==='Enter'){const n=Number(e.currentTarget.value);if(Number.isInteger(n)&&n>=1&&(total===null||n<=Math.ceil(total/65536)))setPage(n-1)}}}/>{total!==null?` / ${Math.max(1,Math.ceil(total/65536))}`:''}</label><button aria-label="Следующая часть" disabled={total!==null&&(page+1)*65536>=total} onClick={()=>setPage(page+1)}>→</button></div>{markdown?<div className="note-content markdown-content"><ReactMarkdown remarkPlugins={[remarkGfm,remarkMath,remarkCurrency,remarkSoftBreaks]} rehypePlugins={[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]]} components={{pre:MarkdownPre,a:({href,children})=><a href={href} target="_blank" rel="noreferrer">{children}</a>}}>{value}</ReactMarkdown></div>:<pre className="file-preview-text">{/\.py$/i.test(filename)?<PythonCode source={value}/>:value}</pre>}</>;
}

export function PdfPreview({url, title, search}: {url:string; title:string; search?:SearchChunk|null}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const viewportRef=useRef<HTMLDivElement>(null);
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null);
  const [page, setPage] = useState(1);
  const [error, setError] = useState('');
  const [searchStatus,setSearchStatus]=useState('');
  const [match,setMatch]=useState<PdfMatch|null>(null);
  const [rects,setRects]=useState<{x:number;y:number;w:number;h:number}[]>([]);
  const [zoom,setZoom]=useState(1);
  const [width,setWidth]=useState(0);
  const [pageSize,setPageSize]=useState({width:1,height:1});
  const navigateToMatch=useRef(false);
  const searchSequence=useRef(0);
  useEffect(()=>{
    const box=viewportRef.current;if(!box)return;
    const update=()=>setWidth(box.clientWidth);update();
    const observer=new ResizeObserver(update);observer.observe(box);return()=>observer.disconnect();
  },[]);
  useEffect(() => {
    setDocument(null); setPage(1); setError(''); setZoom(1);
    const task = getDocument({url});
    let alive = true;
    task.promise.then(pdf => { if (alive) setDocument(pdf); }).catch(() => { if (alive) setError('Не удалось открыть PDF.'); });
    return () => { alive = false; void task.destroy(); };
  }, [url]);
  useEffect(()=>{
    const seq=++searchSequence.current;setMatch(null);setRects([]);
    if(!document||!search?.text||search.field==='description'){setSearchStatus('');return;}
    setSearchStatus('Ищем страницу в PDF…');
    void locatePdfFragment(document,search.text,search.start,()=>searchSequence.current!==seq,
      (p,n)=>setSearchStatus(`Ищем страницу в PDF… ${p} / ${n}`))
      .then(found=>{if(searchSequence.current!==seq)return;if(!found){setSearchStatus('Не удалось найти страницу фрагмента.');return;}navigateToMatch.current=true;setMatch(found);setPage(found.page);setSearchStatus(found.ambiguous?(found.partial?'Найдено несколько частичных совпадений; проверьте страницу.':'Найдено несколько совпадений; показана ближайшая страница.'):found.partial?'Найдено частичное совпадение; проверьте страницу.':'');})
      .catch(()=>{if(searchSequence.current===seq)setSearchStatus('Не удалось прочитать PDF. Попробуйте открыть файл заново.');});
    return()=>{searchSequence.current++};
  },[document,search]);
  useEffect(() => {
    if (!document || !canvas.current) return;
    let alive = true;
    let renderTask: RenderTask | null = null;
    document.getPage(page).then(pdfPage => {
      if (!alive || !canvas.current) return;
      const natural = pdfPage.getViewport({scale: 1});
      const viewport = pdfPage.getViewport({scale: Math.min(Math.max(width-16, 100), 1100) / natural.width * zoom});
      const target = canvas.current;
      const box=viewportRef.current;
      const relativeY=box && target.clientHeight ? box.scrollTop / target.clientHeight : 0;
      // Limit the backing bitmap to 16 million pixels to avoid excessive memory on large pages.
      const outputScale=Math.min(window.devicePixelRatio || 1, 2, Math.sqrt(16_000_000/(viewport.width*viewport.height)));
      target.width = Math.floor(viewport.width*outputScale); target.height = Math.floor(viewport.height*outputScale);
      target.style.width=`${viewport.width}px`; target.style.height=`${viewport.height}px`;
      setPageSize({width:viewport.width,height:viewport.height});
      const highlights=match?.page===page?match.matches.map(index=>{const item=match.items[index];const t=Util.transform(viewport.transform,item.transform);const h=Math.hypot(t[2],t[3]);return {x:t[4],y:t[5]-h,w:Math.abs(item.width*viewport.scale),h};}):[];
      setRects([]);
      renderTask = pdfPage.render({canvas: target, viewport, transform:[outputScale,0,0,outputScale,0,0]});
      return renderTask.promise.then(()=>{const box=viewportRef.current;if(!alive||!box)return;setRects(highlights);if(highlights.length&&navigateToMatch.current){navigateToMatch.current=false;const y=target.getBoundingClientRect().top-box.getBoundingClientRect().top+box.scrollTop+highlights[0].y;box.scrollTop=Math.max(0,y-80)}else box.scrollTop=relativeY*target.clientHeight});
    }).catch(() => { if (alive) setError('Не удалось отобразить страницу PDF.'); });
    return () => { alive = false; renderTask?.cancel(); };
  }, [document, page, match, zoom, width]);
  return <div className="pdf-preview" ref={viewportRef} aria-label={title}>
    {error ? <p role="alert">{error}</p> : !document ? <p role="status">Открываем PDF…</p> : <>
      <div className="pdf-preview-controls"><button aria-label="Предыдущая страница" disabled={page <= 1} onClick={() => {searchSequence.current++;setMatch(null);setSearchStatus('');setPage(page - 1)}}>←</button><span>{page} / {document.numPages}</span><button aria-label="Следующая страница" disabled={page >= document.numPages} onClick={() => {searchSequence.current++;setMatch(null);setSearchStatus('');setPage(page + 1)}}>→</button><button aria-label="Уменьшить PDF" disabled={zoom<=0.5} onClick={()=>setZoom(z=>Math.max(0.5,Math.round((z-0.25)*100)/100))}>−</button><span>{Math.round(zoom*100)}%</span><button aria-label="Увеличить PDF" disabled={zoom>=3} onClick={()=>setZoom(z=>Math.min(3,Math.round((z+0.25)*100)/100))}>+</button></div>
      {searchStatus&&<p role="status">{searchStatus}</p>}
      <div className="pdf-page"><canvas ref={canvas}/>{rects.map((rect,index)=><span className="search-highlight-mark pdf-search-highlight" aria-hidden="true" key={`${searchSequence.current}-${index}`} style={{left:`${rect.x/pageSize.width*100}%`,top:`${rect.y/pageSize.height*100}%`,width:`${rect.w/pageSize.width*100}%`,height:`${rect.h/pageSize.height*100}%`}} />)}</div>
    </>}
  </div>;
}
