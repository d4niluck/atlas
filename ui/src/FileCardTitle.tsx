import React, {useRef, useState} from 'react';
import './file-card-title.css';

/** The extension remains fixed: Atlas uses it to select the file viewer. */
export function FileCardTitle({title,onRename,onError}: {title:string;onRename:(title:string)=>Promise<void>;onError:(message:string)=>void}) {
 const dot=title.lastIndexOf('.');const suffix=dot>0?title.slice(dot):'';
 const [editing,setEditing]=useState(false);const [draft,setDraft]=useState('');const [saving,setSaving]=useState(false);
 const cancelled=useRef(false);
 function begin(){if(saving)return;cancelled.current=false;setDraft(suffix?title.slice(0,-suffix.length):title);setEditing(true)}
 async function finish(){const base=draft.trim();setEditing(false);if(cancelled.current||!base||base+suffix===title)return;setSaving(true);try{await onRename(base+suffix)}catch(error){onError(error instanceof Error?error.message:String(error))}finally{setSaving(false)}}
 return editing?<span className="file-card-title-edit" onPointerDown={e=>e.stopPropagation()} onClick={e=>e.stopPropagation()} onDoubleClick={e=>e.stopPropagation()}>
  <input aria-label="Название файла" autoFocus value={draft} onFocus={e=>e.currentTarget.select()} onChange={e=>setDraft(e.target.value)} onBlur={()=>void finish()} onKeyDown={e=>{if(e.key==='Enter'){e.preventDefault();e.currentTarget.blur()}else if(e.key==='Escape'){e.preventDefault();e.stopPropagation();cancelled.current=true;e.currentTarget.blur()}}}/>{suffix&&<span>{suffix}</span>}
 </span>:<strong className="file-card-title" role="button" tabIndex={0} title="Дважды нажмите, чтобы переименовать файл" aria-label={`Переименовать файл ${title}`} onDoubleClick={e=>{e.stopPropagation();begin()}} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '||e.key==='F2'){e.preventDefault();e.stopPropagation();begin()}}}>{title}</strong>;
}
