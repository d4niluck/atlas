import {VoiceInput} from './VoiceInput';
import {ChatPanel} from './ChatPanel';
import {chatReadiness} from './chatReadiness';
import {appendVoiceText} from './voiceText';
import React, {useEffect, useRef, useState} from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import {remarkCurrency} from './remarkCurrency';
import {remarkSoftBreaks} from './remarkSoftBreaks';
import rehypeKatex from 'rehype-katex';
import {MarkdownLink} from './MarkdownLink';
import {AnswerWaiting} from './AnswerWaiting';
import {citedSources} from './citedSources';
import {MenuSearchResults} from './MenuSearchResults';
import type {SearchChunk, SearchResult} from './SearchResults';
import {selectSearchMode, type SearchMode, type SimpleSearchMode} from './searchMode';
import {searchWithRetry} from './searchRetry';
import {watchRagEvents} from './ragEvents';
import './menu-bar-search.css';

type Workspace = {id:string;name:string;protected?:boolean;available?:boolean};
type Settings = {lexical_enabled:boolean;semantic_enabled:boolean;rag_enabled:boolean;chat_enabled:boolean;agentic:boolean};
type Source = SearchChunk & {id:string;title:string;missing?:boolean};
type Props = {onNavigate:(workspaceId:string,target:{id:string;chunk:SearchChunk|null})=>void|Promise<void>;onOpenSettings?:()=>void|Promise<void>};
type Request = {workspaceId:string;protected:boolean;query:string;mode:SearchMode;agentic:boolean};

export function clearFinishedAttempt(operation:{id:string|null;poll:AbortController|null}){operation.poll?.abort();operation.id=null}
export async function cancelRagAttempt(operation:{id:string|null;prefix:string},fetcher:typeof fetch=fetch){if(!operation.id)return;const response=await fetcher(operation.prefix+'/rag/cancel/'+encodeURIComponent(operation.id),{method:'POST',keepalive:true});if(!response.ok)throw Error()}
export async function reportCancelFailure(operation:{id:string|null;prefix:string},isCurrent:()=>boolean,report:()=>void,fetcher:typeof fetch=fetch){try{await cancelRagAttempt(operation,fetcher)}catch{if(isCurrent())report()}}

export function MenuBarSearch({onNavigate,onOpenSettings}:Props) {
 const [workspaces,setWorkspaces]=useState<Workspace[]>([]);
 const [workspaceId,setWorkspaceId]=useState('');
 const [settings,setSettings]=useState<Settings>({lexical_enabled:true,semantic_enabled:false,rag_enabled:false,chat_enabled:true,agentic:false});
 const [mode,setMode]=useState<SearchMode>('lexical');
 const previous=useRef<SimpleSearchMode>('lexical');
 const initialized=useRef(false);
 const [query,setQuery]=useState('');
 const [results,setResults]=useState<SearchResult[]|null>(null);
 const [answer,setAnswer]=useState<string|null>(null);
 const [sources,setSources]=useState<Source[]>([]);
 const [resultRequest,setResultRequest]=useState<Request|null>(null);
 const [chatMode,setChatMode]=useState(false),[chatStarted,setChatStarted]=useState(false);
 const [chatAccess,setChatAccess]=useState<'checking'|'ready'|'locked'|'unavailable'>('checking'),[chatVisible,setChatVisible]=useState(true),[chatAccessWorkspace,setChatAccessWorkspace]=useState('');
 const chatRevision=useRef(0);
 const [ragStatus,setRagStatus]=useState<{available:boolean;message:string;model?:'e2b'|'e4b'|'nemotron';installed_e2b?:boolean;installed_e4b?:boolean;installed_nemotron?:boolean}>({available:false,message:'Проверяем модель ответа…'});
 const [semanticMessage,setSemanticMessage]=useState('');
 const resultRef=useRef<Request|null>(null);resultRef.current=resultRequest;
 const [privateVisible,setPrivateVisible]=useState(true);
 const visibilityRevision=useRef(0);
 const [searching,setSearching]=useState(false);
 const [waiting,setWaiting]=useState(false);
 const [stopped,setStopped]=useState(false);
 const [error,setError]=useState('');
 const [locked,setLocked]=useState<Request|null>(null);
 const [password,setPassword]=useState('');
 const [unlocking,setUnlocking]=useState(false);
 const active=useRef<{controller:AbortController;poll:AbortController|null;id:string|null;prefix:string}|null>(null);
 const searchGeneration=useRef(0);
 const selected=workspaces.find(w=>w.id===workspaceId);
 function refresh(){
  document.documentElement.dataset.theme=localStorage.getItem('atlas-theme')==='light'?'light':'dark';
  void fetch('/api/workspaces').then(async r=>{if(!r.ok)throw Error();return r.json() as Promise<{workspaces:Workspace[]}>}).then(data=>{
   setWorkspaces(data.workspaces);
   setWorkspaceId(current=>data.workspaces.some(w=>w.id===current)?current:(data.workspaces.find(w=>w.available!==false)?.id??''));
  }).catch(()=>setError('Не удалось загрузить пространства'));
  void fetch('/api/settings/app').then(async r=>{if(!r.ok)throw Error();return r.json() as Promise<Settings>}).then(value=>{setSettings({...value,chat_enabled:value.chat_enabled??true});if(value.chat_enabled===false)setChatMode(false);if(!initialized.current){initialized.current=true;setMode(value.rag_enabled?'rag':value.lexical_enabled?'lexical':'semantic')}}).catch(()=>setError('Не удалось загрузить настройки поиска'));
 }
 useEffect(()=>{
  const blur=()=>{visibilityRevision.current++;setPassword('');if(resultRef.current?.protected)setPrivateVisible(false)};
  const focus=()=>{refresh();const result=resultRef.current;if(!result?.protected)return;const revision=++visibilityRevision.current;setPrivateVisible(false);void fetch('/api/w/'+encodeURIComponent(result.workspaceId)+'/board').then(response=>{if(revision!==visibilityRevision.current||resultRef.current!==result)return;if(response.ok){setPrivateVisible(true);return}stop(false);setResultRequest(null);setResults(null);setAnswer(null);setSources([]);setPassword('');setError(response.status===423?'Пространство заблокировано. Введите пароль для нового поиска.':'Пространство недоступно.')}).catch(()=>{if(revision!==visibilityRevision.current||resultRef.current!==result)return;stop(false);setResultRequest(null);setResults(null);setAnswer(null);setSources([]);setPassword('');setError('Не удалось проверить доступ к пространству.')})};
  refresh();window.addEventListener('focus',focus);window.addEventListener('blur',blur);return()=>{window.removeEventListener('focus',focus);window.removeEventListener('blur',blur);stop(false)}
 },[]);
 // Keep a hidden panel mounted during normal popup dismissal, but never show protected history until access is rechecked.
 useEffect(()=>{
  if(!chatStarted||!workspaceId)return;
  let live=true;
  const check=()=>{const revision=++chatRevision.current;setChatVisible(false);void fetch('/api/w/'+encodeURIComponent(workspaceId)+'/board').then(r=>{if(!live||revision!==chatRevision.current)return;setChatAccessWorkspace(workspaceId);setChatAccess(r.ok?'ready':r.status===423?'locked':'unavailable');setChatVisible(r.ok)}).catch(()=>{if(live&&revision===chatRevision.current){setChatAccess('unavailable');setChatVisible(false)}})};
  const blur=()=>{chatRevision.current++;setChatVisible(false)};
  setChatAccess('checking');check();window.addEventListener('focus',check);window.addEventListener('blur',blur);
  return()=>{live=false;chatRevision.current++;window.removeEventListener('focus',check);window.removeEventListener('blur',blur)};
 },[workspaceId,selected?.protected,chatStarted]);
 useEffect(()=>{
  if(!chatMode||!workspaceId)return;let live=true;const prefix='/api/w/'+encodeURIComponent(workspaceId);
  const refreshStatus=()=>{if(!document.hasFocus())return;void fetch(prefix+'/rag/status').then(r=>{if(!r.ok)throw Error();return r.json()}).then(data=>{if(live)setRagStatus(data)}).catch(()=>{if(live)setRagStatus({available:false,message:'Модель ответа недоступна'})});void fetch(prefix+'/semantic/status').then(r=>{if(!r.ok)throw Error();return r.json()}).then(data=>{if(live)setSemanticMessage(data.message||'')}).catch(()=>{if(live)setSemanticMessage('Поиск по смыслу недоступен')})};
  refreshStatus();window.addEventListener('focus',refreshStatus);const timer=setInterval(refreshStatus,3000);return()=>{live=false;clearInterval(timer);window.removeEventListener('focus',refreshStatus)};
 },[chatMode,workspaceId]);
 async function unlockChat(e:React.FormEvent){e.preventDefault();if(!password||unlocking||!workspaceId)return;setUnlocking(true);setError('');try{const response=await fetch('/api/workspaces/'+encodeURIComponent(workspaceId)+'/unlock',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({password})});const data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось открыть пространство');setPassword('');setChatAccessWorkspace(workspaceId);setChatAccess('ready');setChatVisible(true)}catch(e){setError(e instanceof Error?e.message:'Не удалось открыть пространство')}finally{setUnlocking(false)}}
 function stop(show=true){
  const request=active.current;if(!request)return;
  active.current=null;
  const generation=searchGeneration.current;void reportCancelFailure(request,()=>show&&searchGeneration.current===generation,()=>setError('Не удалось подтвердить остановку ответа'));
  request.poll?.abort();request.controller.abort();
  setSearching(false);setWaiting(false);if(show)setStopped(true);
 }
 async function run(request:Request){
  stop(false);searchGeneration.current++;
  const controller=new AbortController();
  const prefix='/api/w/'+encodeURIComponent(request.workspaceId);
  const operation={controller,poll:null as AbortController|null,id:null as string|null,prefix};active.current=operation;
  const options:RequestInit={method:'POST',headers:{'Content-Type':'application/json'},signal:controller.signal};
  const attempt=()=>{
   operation.poll?.abort();
   const id=request.mode==='rag'?crypto.randomUUID():null;operation.id=id;
   options.body=JSON.stringify({query:request.query,mode:request.mode,...(id?{agentic:request.agentic,request_id:id}:{})});
   if(id){const poll=new AbortController();operation.poll=poll;void watchRagEvents(prefix,id,poll.signal,event=>{
    if(active.current!==operation||operation.id!==id)return;
    if(event.type==='results'){setResults(event.results);setWaiting(false)}
    else if(event.type==='token'){setAnswer(old=>(old??'')+event.text);setWaiting(false)}
    else if(event.type==='reset')setAnswer(null);
   }).catch(e=>{if(!poll.signal.aborted&&active.current===operation)setError(e instanceof Error?e.message:'Не удалось получить ход ответа')});}
  };
  setLocked(null);setPassword('');setPrivateVisible(true);setResultRequest(request);setResults(null);setAnswer(null);setSources([]);setError('');setWaiting(false);setStopped(false);setSearching(true);
  attempt();
  try{
   const response=await searchWithRetry(prefix+(request.mode==='rag'?'/rag/search':'/search'),options,()=>{clearFinishedAttempt(operation);setWaiting(true)},attempt);
   const data=await response.json();if(active.current!==operation)return;
   if(response.status===423){setLocked(request);setPassword('');return}
   if(!response.ok)throw Error(data.error||'Не удалось выполнить поиск');
   setResults(data.results);setAnswer(data.answer??null);setSources(data.sources??[]);
   if(data.error&&data.error!=='Ответ может быть неполным: проверьте найденные фрагменты')setError(data.error);
  }catch(e){if(active.current===operation&&!controller.signal.aborted)setError(e instanceof Error?e.message:'Не удалось выполнить поиск')}
  finally{operation.poll?.abort();if(active.current===operation){active.current=null;setSearching(false);setWaiting(false)}}
 }
 async function unlock(e:React.FormEvent){
  e.preventDefault();if(!locked||!password||unlocking)return;setUnlocking(true);setError('');
  try{const response=await fetch('/api/workspaces/'+encodeURIComponent(locked.workspaceId)+'/unlock',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({password})});const data=await response.json();if(!response.ok)throw Error(data.error||'Не удалось открыть пространство');const request=locked;setLocked(null);setPassword('');void run(request)}catch(e){setError(e instanceof Error?e.message:'Не удалось открыть пространство')}finally{setUnlocking(false)}
 }
 function submit(e:React.FormEvent){e.preventDefault();if(!query.trim()||!workspaceId)return;if(selected?.available===false){setError('Пространство недоступно. Подключите носитель в Atlas.');return}const enabled=mode==='hybrid'?settings.lexical_enabled&&settings.semantic_enabled:settings[`${mode}_enabled` as 'lexical_enabled'|'semantic_enabled'|'rag_enabled'];if(!enabled){setError('Включите этот режим в настройках поиска Atlas.');return}void run({workspaceId,protected:!!selected?.protected,query:query.trim(),mode,agentic:settings.agentic})}
 function selectSimple(value:'lexical'|'semantic'){setChatMode(false);const next=selectSearchMode(mode,previous.current,value);previous.current=next;setMode(next)}
 function action(fn:()=>void|Promise<void>){void Promise.resolve().then(fn).catch(e=>setError(e instanceof Error?e.message:'Не удалось открыть Atlas'))}
 const isRag=resultRequest?.mode==='rag';
 const resizeQueue=useRef<Promise<void>>(Promise.resolve());
 useEffect(()=>{const tauri=(window as any).__TAURI__;if(!tauri?.core)return;let stale=false;resizeQueue.current=resizeQueue.current.catch(()=>{}).then(async()=>{if(stale)return;try{await tauri.core.invoke('set_quick_search_mode',{wide:!chatMode&&mode==='rag'})}catch{if(!stale)setError('Не удалось изменить размер окна поиска')}});return()=>{stale=true}},[mode,chatMode]);
 return <main className={'menu-search '+(!chatMode&&mode==='rag'?'menu-search-wide':'menu-search-compact')}>
  <header className="menu-search-head"><label className="menu-search-workspace"><span className="sr-only">Пространство</span><select aria-label="Пространство" value={workspaceId} onChange={e=>{setWorkspaceId(e.target.value);setLocked(null);setPassword('')}}><option value="" disabled>Выберите пространство</option>{workspaces.map(w=><option key={w.id} value={w.id}>{w.protected?'🔒 ':''}{w.name}{w.available===false?' · недоступно':''}</option>)}</select></label>{onOpenSettings&&<button type="button" className="menu-search-icon" aria-label="Настройки поиска" title="Настройки поиска" onClick={()=>action(onOpenSettings)}>⚙</button>}</header>
  <div className="menu-search-query-row"><div className="menu-search-controls"><div className="menu-search-modes" role="group" aria-label="Режим поиска"><button type="button" aria-pressed={!chatMode&&(mode==='lexical'||mode==='hybrid')} disabled={!settings.lexical_enabled} title="По словам" aria-label="По словам" onClick={()=>selectSimple('lexical')}>T</button><button type="button" aria-pressed={!chatMode&&(mode==='semantic'||mode==='hybrid')} disabled={!settings.semantic_enabled} title="По смыслу" aria-label="По смыслу" onClick={()=>selectSimple('semantic')}><svg className="menu-search-semantic-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="10" cy="10" r="6"/><path d="m14.5 14.5 5 5M10 7v6M7 10h6"/></svg></button><button type="button" aria-pressed={!chatMode&&mode==='rag'} disabled={!settings.rag_enabled} title="Умный поиск" aria-label="Умный поиск" onClick={()=>{setChatMode(false);if(mode!=='rag')previous.current=mode;setMode('rag')}}>🧠</button><button type="button" aria-pressed={chatMode} disabled={!settings.chat_enabled} title="Чат" aria-label="Чат" onClick={()=>{stop(false);setChatStarted(true);setChatMode(true)}}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="M4 5h16v11H9l-5 4z"/></svg></button></div></div>
  {!chatMode&&<form className="menu-search-form" onSubmit={submit}><div className="voice-field voice-field-reverse"><input aria-label="Поисковый запрос" value={query} onChange={e=>setQuery(e.target.value)} placeholder="Найти в пространстве…"/><VoiceInput onText={text=>setQuery(query=>appendVoiceText(query,text))}/></div><button type="submit" disabled={!workspaceId||!query.trim()} aria-label="Искать" title="Искать">⌕</button>{searching&&<button type="button" className="menu-search-stop" aria-label="Остановить поиск" title="Остановить поиск" onClick={()=>stop()}>■</button>}</form>}</div>
  {error&&<p className="menu-search-error" role="alert">{error}</p>}
  {locked&&<form className="menu-search-unlock" onSubmit={unlock}><label>Пароль пространства<input type="password" value={password} onChange={e=>setPassword(e.target.value)} autoFocus autoComplete="off"/></label><button type="submit" disabled={!password||unlocking}>Открыть</button><button type="button" onClick={()=>{setLocked(null);setPassword('')}}>Отмена</button></form>}
  {chatMode&&chatAccess==='locked'&&<form className="menu-search-unlock" onSubmit={unlockChat}><label>Пароль пространства<input type="password" value={password} onChange={e=>setPassword(e.target.value)} autoComplete="off"/></label><button type="submit" disabled={!password||unlocking}>Открыть</button></form>}
  {chatMode&&chatAccess==='unavailable'&&<p className="menu-search-error" role="alert">Пространство недоступно.</p>}
  {chatStarted&&settings.chat_enabled&&workspaceId&&chatAccess==='ready'&&chatAccessWorkspace===workspaceId&&<div className="menu-chat-wrapper" hidden={!chatMode||!chatVisible}><ChatPanel key={workspaceId} prefix={'/api/w/'+encodeURIComponent(workspaceId)} objectIds={[]} visible={chatMode&&chatVisible} readiness={chatReadiness(settings,ragStatus,semanticMessage)} onOpenSettings={()=>action(()=>onOpenSettings?.())} onOpen={(id,chunk)=>action(()=>onNavigate(workspaceId,{id,chunk}))}/></div>}
  {!chatMode&&resultRequest&&(!resultRequest.protected||privateVisible)&&<div className="menu-search-result-area"><div className={'menu-search-output'+(isRag?' menu-search-output-rag':'')}>
   {isRag&&<section className="menu-search-answer" aria-label="Ответ"><h2>Ответ</h2>{searching&&!answer&&<AnswerWaiting/>}{answer&&<div className="menu-search-markdown"><ReactMarkdown remarkPlugins={[remarkGfm,remarkMath,remarkCurrency,remarkSoftBreaks]} rehypePlugins={[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]]} components={{a:MarkdownLink}}>{answer}</ReactMarkdown></div>}{stopped&&<small>Остановлено</small>}{citedSources(answer,sources).map(({number,source})=><button type="button" className="menu-search-citation" key={number} disabled={source.missing} onClick={()=>action(()=>onNavigate(resultRequest.workspaceId,{id:source.id,chunk:source.stale?null:source}))}>[{number}] {source.title}</button>)}</section>}
   <section className="menu-search-results" aria-label="Результаты поиска">{isRag&&<h2>Фрагменты</h2>}{waiting&&<p role="status">Подготавливаем поиск…</p>}<MenuSearchResults results={results} onOpen={(id,chunk)=>action(()=>onNavigate(resultRequest.workspaceId,{id,chunk}))}/>{searching&&results===null&&!isRag&&!waiting&&<p role="status">Ищем…</p>}</section>
  </div></div>}
 </main>;
}
