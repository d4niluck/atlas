import React,{useEffect,useRef} from 'react';

type Props={x:number;y:number;start:boolean;end:boolean;onToggle:(side:'start'|'end')=>void;onClose:()=>void};
export function EdgeMenu({x,y,start,end,onToggle,onClose}:Props){
 const ref=useRef<HTMLDivElement>(null);
 useEffect(()=>{ref.current?.querySelector<HTMLButtonElement>('button')?.focus();const outside=(event:PointerEvent)=>{if(!ref.current?.contains(event.target as Node))onClose()};const key=(event:KeyboardEvent)=>{if(event.key==='Escape'){event.preventDefault();onClose()}};document.addEventListener('pointerdown',outside,true);document.addEventListener('keydown',key);return()=>{document.removeEventListener('pointerdown',outside,true);document.removeEventListener('keydown',key)}},[onClose]);
 return <div ref={ref} className="card-menu" role="menu" style={{left:Math.max(8,Math.min(x,window.innerWidth-220)),top:Math.max(8,Math.min(y,window.innerHeight-110))}} onContextMenu={event=>event.preventDefault()}>
  <button role="menuitemcheckbox" aria-checked={start} onClick={()=>onToggle('start')}>{start?'✓ ':'　'}Стрелка в начале</button>
  <button role="menuitemcheckbox" aria-checked={end} onClick={()=>onToggle('end')}>{end?'✓ ':'　'}Стрелка в конце</button>
 </div>;
}
