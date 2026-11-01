/// <reference types="vite/client" />
import {useEffect, useRef, useState} from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {MarkdownPre} from './MermaidPre';
import {getDocument, GlobalWorkerOptions} from './pdfCompat';
import type {PDFDocumentProxy, RenderTask} from 'pdfjs-dist/legacy/build/pdf.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import './file-preview-layout.css';

GlobalWorkerOptions.workerSrc = workerUrl;


export function PlainFilePreview({url, markdown=false}: {url:string; markdown?:boolean}) {
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
 return status?<p role="status">{status}</p>:<><div className="pdf-preview-controls"><button aria-label="Предыдущая часть" disabled={page===0} onClick={()=>setPage(page-1)}>←</button><label>Часть <input style={{width:72}} aria-label="Номер части" type="number" min="1" max={total!==null?Math.max(1,Math.ceil(total/65536)):undefined} defaultValue={page+1} key={page} onKeyDown={e=>{if(e.key==='Enter'){const n=Number(e.currentTarget.value);if(Number.isInteger(n)&&n>=1&&(total===null||n<=Math.ceil(total/65536)))setPage(n-1)}}}/>{total!==null?` / ${Math.max(1,Math.ceil(total/65536))}`:''}</label><button aria-label="Следующая часть" disabled={total!==null&&(page+1)*65536>=total} onClick={()=>setPage(page+1)}>→</button></div>{markdown?<div className="note-content markdown-content"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{pre:MarkdownPre,a:({href,children})=><a href={href} target="_blank" rel="noreferrer">{children}</a>}}>{value}</ReactMarkdown></div>:<pre className="file-preview-text">{value}</pre>}</>;
}

export function PdfPreview({url, title}: {url:string; title:string}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null);
  const [page, setPage] = useState(1);
  const [error, setError] = useState('');
  useEffect(() => {
    setDocument(null); setPage(1); setError('');
    const task = getDocument({url});
    let alive = true;
    task.promise.then(pdf => { if (alive) setDocument(pdf); }).catch(() => { if (alive) setError('Не удалось открыть PDF.'); });
    return () => { alive = false; void task.destroy(); };
  }, [url]);
  useEffect(() => {
    if (!document || !canvas.current) return;
    let alive = true;
    let renderTask: RenderTask | null = null;
    document.getPage(page).then(pdfPage => {
      if (!alive || !canvas.current) return;
      const viewport = pdfPage.getViewport({scale: 1.35});
      const target = canvas.current;
      target.width = viewport.width; target.height = viewport.height;
      renderTask = pdfPage.render({canvas: target, viewport});
      return renderTask.promise;
    }).catch(() => { if (alive) setError('Не удалось отобразить страницу PDF.'); });
    return () => { alive = false; renderTask?.cancel(); };
  }, [document, page]);
  return <div className="pdf-preview" aria-label={title}>
    {error ? <p role="alert">{error}</p> : !document ? <p role="status">Открываем PDF…</p> : <>
      <div className="pdf-preview-controls"><button aria-label="Предыдущая страница" disabled={page <= 1} onClick={() => setPage(page - 1)}>←</button><span>{page} / {document.numPages}</span><button aria-label="Следующая страница" disabled={page >= document.numPages} onClick={() => setPage(page + 1)}>→</button></div>
      <canvas ref={canvas}/>
    </>}
  </div>;
}
