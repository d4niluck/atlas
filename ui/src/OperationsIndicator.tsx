import React,{useEffect,useRef,useState} from 'react';
import './operations-indicator.css';
type Update={id:string;name:string;operations:string[]};
export function OperationsIndicator(){
 const [entries,setEntries]=useState<Record<string,Update>>({});
 const [open,setOpen]=useState(false);
 const container=useRef<HTMLDivElement>(null),trigger=useRef<HTMLButtonElement>(null);
 useEffect(()=>{if(!open)return;const outside=(event:PointerEvent)=>{if(!container.current?.contains(event.target as Node))setOpen(false)};const key=(event:KeyboardEvent)=>{if(event.key==='Escape'){event.stopPropagation();setOpen(false);trigger.current?.focus()}};document.addEventListener('pointerdown',outside);document.addEventListener('keydown',key);return()=>{document.removeEventListener('pointerdown',outside);document.removeEventListener('keydown',key)}},[open]);
 const [uploads,setUploads]=useState(0);
 useEffect(()=>{let stopped=false;let controller:AbortController|null=null;const poll=async()=>{if(document.visibilityState==='hidden'||controller)return;controller=new AbortController();try{const response=await fetch('/api/agent-operations',{signal:controller.signal});if(!response.ok){if(!stopped)setUploads(0);return}const value=await response.json();if(!stopped)setUploads(Number.isSafeInteger(value.uploads)&&value.uploads>0?value.uploads:0)}catch{if(!stopped)setUploads(0)}finally{controller=null}};void poll();const timer=setInterval(()=>void poll(),2000);return()=>{stopped=true;clearInterval(timer);controller?.abort()}},[]);
 useEffect(()=>{const update=(event:Event)=>{const item=(event as CustomEvent<Update>).detail;setEntries(current=>{const next={...current};if(item.operations.length)next[item.id]=item;else delete next[item.id];return next})};window.addEventListener('atlas-ui-operations',update);return()=>window.removeEventListener('atlas-ui-operations',update)},[]);
 const active=Object.values(entries);
 useEffect(()=>{if(!active.length&&!uploads)setOpen(false)},[active.length,uploads]);
 if(!active.length&&!uploads)return null;
 return <div className="operations-indicator" ref={container}><button ref={trigger} type="button" aria-label="Текущие операции" aria-expanded={open} title="Текущие операции" onClick={()=>setOpen(v=>!v)}><span className="operations-spinner"/></button>{open&&<div className="operations-popover" role="status"><strong>Сейчас выполняется</strong>{uploads>0&&<div><span className="operations-dot"/>Загрузка агентами · {uploads}</div>}{active.flatMap(entry=>entry.operations.map(operation=><div key={entry.id+operation}><span className="operations-dot"/>{operation} · {entry.name}</div>))}</div>}</div>
}
