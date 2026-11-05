import React, {useRef, useState} from 'react';

export function StackTitle({title, count, onRename}: {title:string; count:number; onRename:(title:string)=>void}) {
 const [editing,setEditing]=useState(false);
 const [draft,setDraft]=useState(title);
 const cancelled=useRef(false);
 const begin=()=>{cancelled.current=false;setDraft(title);setEditing(true)};
 const finish=()=>{const next=draft.trim();setEditing(false);if(!cancelled.current&&next&&next!==title)onRename(next)};
 return editing
  ? <input className="stack-title-input" aria-label="Название группы" value={draft} autoFocus onFocus={event=>event.currentTarget.select()}
      onChange={event=>setDraft(event.target.value)} onBlur={finish}
      onPointerDown={event=>event.stopPropagation()} onClick={event=>event.stopPropagation()} onDoubleClick={event=>event.stopPropagation()}
      onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();event.currentTarget.blur()}else if(event.key==='Escape'){event.preventDefault();event.stopPropagation();cancelled.current=true;event.currentTarget.blur()}}}/>
  : <span className="stack-title" role="button" tabIndex={0} aria-label={`Переименовать группу ${title}`} title="Дважды нажмите, чтобы переименовать"
      onDoubleClick={event=>{event.stopPropagation();begin()}} onKeyDown={event=>{if(event.key==='Enter'||event.key===' '||event.key==='F2'){event.preventDefault();event.stopPropagation();begin()}}}>{title} · {count}</span>;
}
