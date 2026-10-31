import React, {useState} from 'react';
import './control-contrast.css';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {MarkdownEditor} from './MarkdownEditor';
import {MarkdownLink} from './MarkdownLink';
import {MarkdownSearchExcerpt} from './MarkdownSearchExcerpt';
import {MarkdownPre} from './MermaidPre';
import type {SearchChunk} from './SearchResults';
import {nextNoteMode} from './note-mode';

type Props={title:string;text:string;highlight:SearchChunk|null;onTitle:(value:string)=>void;onText:(value:string)=>void;onClose:()=>void};
export function NoteEditor({title,text,highlight,onTitle,onText,onClose}:Props){
 const [mode,setMode]=useState({expanded:false,editing:false});
 const {expanded,editing}=mode;
 return <div className="overlay" onClick={onClose}>
  <section className={'editor note-editor'+(expanded?' fullscreen':'')} onClick={e=>e.stopPropagation()}>
   <div className="editor-top"><span className="eyebrow">ЗАМЕТКА</span><div>
    <button type="button" onClick={()=>setMode(current=>nextNoteMode(current,'toggle-edit'))}>{editing?'Читать':'Редактировать'}</button>
    <button type="button" onClick={()=>setMode(current=>nextNoteMode(current,'expand'))}>{expanded?'Свернуть':'На весь экран'}</button>
    <button type="button" onClick={onClose} aria-label="Закрыть заметку">×</button>
   </div></div>
   <input className="title-input" aria-label="Название заметки" value={title} onChange={e=>onTitle(e.target.value)}/>
   <div className={'note-split'+(expanded&&editing?' split-expanded':'')}>
    {editing&&<MarkdownEditor text={text} onChange={onText}/>}
    {(expanded||!editing)&&<div className="note-preview">{expanded&&editing&&<span className="eyebrow">ПРЕДПРОСМОТР</span>}<div className="note-content markdown-content">
     {highlight?.start!=null&&highlight.end!=null?<MarkdownSearchExcerpt text={text} start={highlight.start} end={highlight.end} components={{a:MarkdownLink,pre:MarkdownPre}}/>:<ReactMarkdown components={{a:MarkdownLink,pre:MarkdownPre}} remarkPlugins={[remarkGfm]}>{text||'Здесь пока пусто.'}</ReactMarkdown>}
    </div></div>}
   </div>
  </section>
 </div>;
}
