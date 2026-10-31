import React, {useEffect, useRef} from 'react';

type Props={x:number;y:number;canCopy:boolean;canExtract:boolean;onCopy:()=>void;onDelete:()=>void;onExtract:()=>void;onLink:()=>void;onClose:()=>void};
export function CardMenu({x,y,canCopy,canExtract,onCopy,onDelete,onExtract,onLink,onClose}:Props){
 const ref=useRef<HTMLDivElement>(null);
 useEffect(()=>{ref.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();const outside=(event:PointerEvent)=>{if(!ref.current?.contains(event.target as Node))onClose()};const key=(event:KeyboardEvent)=>{if(event.key==='Escape'){event.preventDefault();onClose()}};document.addEventListener('pointerdown',outside,true);document.addEventListener('keydown',key);return()=>{document.removeEventListener('pointerdown',outside,true);document.removeEventListener('keydown',key)}},[onClose]);
 const run=(action:()=>void)=>{onClose();action()};
 return <div ref={ref} className="card-menu" role="menu" style={{left:Math.max(8,Math.min(x,window.innerWidth-220)),top:Math.max(8,Math.min(y,window.innerHeight-200))}} onContextMenu={event=>event.preventDefault()}>
  <button role="menuitem" disabled={!canCopy} onClick={()=>run(onCopy)}>Копировать</button>
  <button role="menuitem" onClick={()=>run(onLink)}>Создать связь</button>
  {canExtract&&<button role="menuitem" onClick={()=>run(onExtract)}>Извлечь из группы</button>}
  <button role="menuitem" onClick={()=>run(onDelete)}>Удалить</button>
 </div>;
}
