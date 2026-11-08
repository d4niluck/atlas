import React, {useEffect, useRef, useState} from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import {remarkCurrency} from './remarkCurrency';
import {remarkSoftBreaks} from './remarkSoftBreaks';
import {MarkdownLink} from './MarkdownLink';
import {MarkdownCodePre} from './MarkdownCodePre';
import {watchRagEvents} from './ragEvents';
import {citedSources} from './citedSources';
import type {SearchChunk} from './SearchResults';
import './chat-panel.css';

type Source = SearchChunk & {id:string;title:string;missing?:boolean};
type Message = {role:'user'|'assistant';content:string;sources?:Source[];steps?:string[]};
type Operation = {id:string;request:AbortController;poll:AbortController};

export function ChatPanel({prefix,objectIds,visible,onOpen}:{prefix:string;objectIds:string[];visible:boolean;onOpen:(id:string,chunk:SearchChunk)=>void}) {
 const [messages,setMessages]=useState<Message[]>([]),[draft,setDraft]=useState(''),[stream,setStream]=useState('');
 const [steps,setSteps]=useState<string[]>([]),[expanded,setExpanded]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const [confirmClear,setConfirmClear]=useState(false),[loaded,setLoaded]=useState(false);
 const operation=useRef<Operation|null>(null),bottom=useRef<HTMLDivElement>(null),messageList=useRef<HTMLDivElement>(null),follow=useRef(true);
 const load=async(signal?:AbortSignal)=>{const response=await fetch(prefix+'/chat/history',{signal});if(!response.ok)throw Error('Не удалось загрузить историю чата');const value=await response.json();if(!Array.isArray(value.messages)||!value.messages.every((m:Message)=>['user','assistant'].includes(m.role)&&typeof m.content==='string'))throw Error('История чата недоступна');setMessages(value.messages);setLoaded(true)};
 useEffect(()=>{const controller=new AbortController();void load(controller.signal).catch(e=>{if(!controller.signal.aborted)setError(e.message)});return()=>{controller.abort();const current=operation.current;if(current){current.request.abort();current.poll.abort();void fetch(prefix+'/rag/cancel/'+encodeURIComponent(current.id),{method:'POST',keepalive:true}).catch(()=>{})}}},[prefix]);
 useEffect(()=>{if(visible&&follow.current)bottom.current?.scrollIntoView({block:'end',behavior:'smooth'})},[messages,stream,steps,visible]);
 async function stop(){const current=operation.current;if(!current)return;try{const response=await fetch(prefix+'/rag/cancel/'+encodeURIComponent(current.id),{method:'POST',keepalive:true});if(!response.ok)throw Error();current.request.abort();current.poll.abort()}catch{setError('Не удалось остановить ответ')}}
 async function send(e:React.FormEvent){
  e.preventDefault();const query=draft.trim();if(!query||busy||operation.current||!loaded)return;follow.current=true;
  const current={id:crypto.randomUUID(),request:new AbortController(),poll:new AbortController()};operation.current=current;
  setDraft('');setBusy(true);setError('');setStream('');setSteps([]);setExpanded(true);setMessages(m=>[...m,{role:'user',content:query}]);
  void watchRagEvents(prefix,current.id,current.poll.signal,event=>{if(operation.current!==current)return;if(event.type==='token')setStream(s=>s+event.text);else if(event.type==='reset')setStream('');else if(event.type==='progress')setSteps(s=>[...s,event.text]);else if(event.type==='error')setError(event.error)}).catch(e=>{if(!current.poll.signal.aborted)setError(e instanceof Error?e.message:'Не удалось получить ход ответа')});
  try{const response=await fetch(prefix+'/chat/send',{method:'POST',headers:{'Content-Type':'application/json'},signal:current.request.signal,body:JSON.stringify({query,request_id:current.id,object_ids:objectIds.length?objectIds:null})});const data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось сформировать ответ');await load();setStream('');setSteps([]);setExpanded(false)}catch(e){if(!current.request.signal.aborted)setError(e instanceof Error?e.message:'Не удалось сформировать ответ');else setError('Формирование ответа остановлено');setDraft(query);setStream('');setSteps([]);setExpanded(false);try{await load()}catch{setError('Не удалось обновить историю чата')}}finally{current.poll.abort();if(operation.current===current){operation.current=null;setBusy(false)}}
 }
 async function clear(){setError('');try{const response=await fetch(prefix+'/chat/clear',{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});if(!response.ok)throw Error('Не удалось очистить контекст');await load();setSteps([]);setStream('');setConfirmClear(false)}catch(e){setError(e instanceof Error?e.message:'Не удалось очистить контекст')}}
 return <section className="chat-panel" aria-label="Чат">
  <div className="chat-toolbar"><span>Чат</span><button type="button" disabled={busy||!loaded} onClick={()=>setConfirmClear(true)} title="Очистить контекст" aria-label="Очистить контекст"><svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5"/></svg></button></div>
  {confirmClear&&<div className="chat-confirm" role="alertdialog" aria-label="Очистить контекст"><p>Очистить контекст чата? История останется доступной.</p><div><button type="button" onClick={()=>setConfirmClear(false)}>Отмена</button><button type="button" onClick={()=>void clear()}>Очистить</button></div></div>}
  <div ref={messageList} onScroll={()=>{const list=messageList.current;if(list)follow.current=list.scrollHeight-list.scrollTop-list.clientHeight<80}} className="chat-messages" role="log" aria-label="Сообщения чата">{!messages.length&&!busy&&<p className="chat-empty">Задайте вопрос о вашем пространстве</p>}{messages.map((m,i)=><article key={i} className={'chat-message chat-message-'+m.role}><div className="chat-message-role">{m.role==='user'?'Вы':'Atlas'}</div>{m.role==='assistant'&&Array.isArray(m.steps)&&m.steps.length>0&&<details className="chat-progress"><summary>Ход работы</summary><ol>{m.steps.map((step,j)=><li key={j}>{step}</li>)}</ol></details>}<div className="markdown-content"><ReactMarkdown components={{a:MarkdownLink,pre:MarkdownCodePre}} remarkPlugins={[remarkGfm,remarkMath,remarkCurrency,remarkSoftBreaks]} rehypePlugins={[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]]}>{m.content}</ReactMarkdown></div>{citedSources(m.content,m.sources??[]).length>0&&<ol className="chat-sources">{citedSources(m.content,m.sources??[]).map(({source,number})=><li key={number}><button type="button" disabled={source.missing||source.stale} onClick={()=>onOpen(source.id,source)}>[{number}] {source.title}</button></li>)}</ol>}</article>)}
   {(busy||steps.length>0)&&<details className="chat-progress" open={expanded} onToggle={e=>setExpanded(e.currentTarget.open)}><summary>{busy?'Atlas работает…':'Ход работы'}</summary><ol>{steps.map((step,i)=><li key={i}>{step}</li>)}</ol></details>}
   {stream&&<article className="chat-message chat-message-assistant"><div className="chat-message-role">Atlas</div><div className="markdown-content"><ReactMarkdown components={{a:MarkdownLink,pre:MarkdownCodePre}} remarkPlugins={[remarkGfm,remarkMath,remarkCurrency,remarkSoftBreaks]} rehypePlugins={[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]]}>{stream}</ReactMarkdown></div></article>}<div ref={bottom}/></div>
  {error&&<p className="chat-error" role="alert">{error}</p>}{error&&!loaded&&<button type="button" className="chat-retry" onClick={()=>{setError('');void load().catch(e=>setError(e instanceof Error?e.message:'Не удалось загрузить историю чата'))}}>Повторить</button>}
  <form className="chat-compose" onSubmit={send}><textarea value={draft} onChange={e=>setDraft(e.target.value)} placeholder="Напишите сообщение…" aria-label="Сообщение" onKeyDown={e=>{if(e.key==='Enter'&&!e.shiftKey&&!e.nativeEvent.isComposing){e.preventDefault();e.currentTarget.form?.requestSubmit()}}}/>{busy?<button type="button" onClick={()=>void stop()} aria-label="Остановить ответ" title="Остановить ответ">■</button>:<button type="submit" disabled={!draft.trim()||!loaded} aria-label="Отправить сообщение" title="Отправить сообщение">↑</button>}</form>
 </section>;
}
