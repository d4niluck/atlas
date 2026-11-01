import React,{useEffect,useRef} from 'react';
import {createPortal} from 'react-dom';
import './workspace-access.css';
export function WorkspaceAccessDialog({title,onClose,onSubmit,busy,children}:{title:string;onClose:()=>void;onSubmit:(e:React.FormEvent)=>void;busy:boolean;children:React.ReactNode}){
 const form=useRef<HTMLFormElement>(null),close=useRef(onClose),pending=useRef(busy);close.current=onClose;pending.current=busy;
 useEffect(()=>{const previous=document.activeElement as HTMLElement|null;form.current?.querySelector<HTMLInputElement>('input:not([type=checkbox])')?.focus();function key(e:KeyboardEvent){if(e.key==='Escape'){e.preventDefault();if(!pending.current)close.current();return}if(e.key!=='Tab'||!form.current)return;const items=Array.from(form.current.querySelectorAll<HTMLElement>('input:not(:disabled),button:not(:disabled)'));if(!items.length){e.preventDefault();return}if(e.shiftKey&&document.activeElement===items[0]){e.preventDefault();items[items.length-1].focus()}else if(!e.shiftKey&&document.activeElement===items[items.length-1]){e.preventDefault();items[0].focus()}}document.addEventListener('keydown',key);return()=>{document.removeEventListener('keydown',key);previous?.focus()}},[]);
 return createPortal(<div className="access-overlay"><form ref={form} className="access-dialog" role="dialog" aria-modal="true" aria-labelledby="access-title" onSubmit={onSubmit}><h2 id="access-title">{title}</h2>{children}</form></div>,document.body);
}
