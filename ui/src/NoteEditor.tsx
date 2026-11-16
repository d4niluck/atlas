import {useEditorResize} from './EditorResize';
import React, {useEffect,useState} from 'react';
import './control-contrast.css';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import {remarkCurrency} from './remarkCurrency';
import rehypeKatex from 'rehype-katex';
import {remarkSoftBreaks} from './remarkSoftBreaks';
import {MarkdownEditor} from './MarkdownEditor';
import {MarkdownLink} from './MarkdownLink';
import {MarkdownSearchExcerpt} from './MarkdownSearchExcerpt';
import {MarkdownCodePre as MarkdownPre} from './MarkdownCodePre';
import type {SearchChunk} from './SearchResults';
import {nextNoteMode} from './note-mode';

type Props={searchJumpRevision:number;searchControl?:React.ReactNode;title:string;text:string;highlight:SearchChunk|null;onTitle:(value:string)=>void;onText:(value:string)=>void;onClose:()=>void;onFinishEditing:()=>Promise<boolean>;onStartEditing?:()=>Promise<boolean>;saving?:boolean;recovery?:{id:string;title:string;text:string}[];onRecover?:(draft:{id:string;title:string;text:string})=>void;onBlurEditor:()=>void};
export function NoteEditor({searchJumpRevision,searchControl,title,text,highlight,onTitle,onText,onClose,onFinishEditing,onStartEditing,saving,recovery,onRecover,onBlurEditor}:Props){
 const [mode,setMode]=useState({expanded:false,editing:false});
 const {expanded,editing}=mode;
 const resizeRef=useEditorResize(expanded);
 // Keep unsaved text in the editor while revealing the selected passage.
 useEffect(()=>{setMode(current=>current.editing&&!current.expanded?{...current,expanded:true}:current)},[searchJumpRevision]);
 return <div className="overlay" onClick={onClose}>
  <section ref={resizeRef} tabIndex={0} aria-label="Просмотр. Для изменения размера: Alt Shift и стрелки" className={'editor note-editor'+(expanded?' fullscreen':'')} onBlur={e=>{if(e.relatedTarget&&!e.currentTarget.contains(e.relatedTarget as Node))onBlurEditor()}} style={!expanded?{height:`min(${Number(localStorage.getItem('atlas-editor-height'))||640}px, 85vh)`,width:`min(${Number(localStorage.getItem('atlas-editor-width'))||900}px, 95vw)`}:undefined} onClick={e=>e.stopPropagation()}>
   <div className="editor-top"><div className="editor-meta"><span className="eyebrow">ЗАМЕТКА</span>{searchControl}</div><div>
    <button type="button" disabled={saving} onClick={async()=>{if(editing&&!await onFinishEditing())return;if(!editing&&onStartEditing&&!await onStartEditing())return;setMode(current=>nextNoteMode(current,'toggle-edit'))}}>{editing?'Читать':'Редактировать'}</button>
    <button type="button" onClick={()=>setMode(current=>nextNoteMode(current,'expand'))}>{expanded?'Свернуть':'На весь экран'}</button>
    <button type="button" onClick={onClose} aria-label="Закрыть заметку">×</button>
   </div></div>
   {editing&&!!recovery?.length&&<section aria-label="Резервные черновики"><p>Найдены резервные черновики. Выберите для восстановления:</p>{recovery.map((draft,index)=><button key={draft.id} disabled={saving} type="button" onClick={()=>onRecover?.(draft)}>Черновик {index+1}: {draft.title}</button>)}</section>}
   <input className="title-input" aria-label="Название заметки" value={title} readOnly={saving||!!onStartEditing&&!editing} onFocus={async()=>{if(!editing&&onStartEditing&&await onStartEditing())setMode(current=>({...current,editing:true}))}} onChange={e=>onTitle(e.target.value)}/>
   <div className={'note-split'+(expanded&&editing?' split-expanded':'')}>
    {editing&&<MarkdownEditor text={text} onChange={onText}/>}
    {(expanded||!editing)&&<div className="note-preview">{expanded&&editing&&<span className="eyebrow">ПРЕДПРОСМОТР</span>}<div className="note-content markdown-content">
     {highlight?.start!=null&&highlight.end!=null?<MarkdownSearchExcerpt key={searchJumpRevision} text={text} start={highlight.start} end={highlight.end} components={{a:MarkdownLink,pre:MarkdownPre}}/>:<ReactMarkdown components={{a:MarkdownLink,pre:MarkdownPre}} remarkPlugins={[remarkGfm,remarkMath,remarkCurrency,remarkSoftBreaks]} rehypePlugins={[[rehypeKatex,{trust:false,maxExpand:500,maxSize:20}]]}>{text||'Здесь пока пусто.'}</ReactMarkdown>}
    </div></div>}
   </div>

  </section>
 </div>;
}
