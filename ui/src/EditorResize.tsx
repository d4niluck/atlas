import React, {useRef} from 'react';

export function EditorResize(){
 const drag=useRef<{y:number;height:number}|null>(null);
 function apply(el:HTMLElement,height:number){const section=el.parentElement;if(!section)return;const value=Math.round(Math.max(300,Math.min(window.innerHeight*.85,height)));section.style.height=`${value}px`;return value}
 function save(el:HTMLElement){const height=el.parentElement?.offsetHeight;if(height)localStorage.setItem('atlas-editor-height',String(height));}
 return <div className="editor-resize" role="separator" aria-label="Изменить высоту окна" aria-orientation="horizontal" tabIndex={0}
  onPointerDown={e=>{e.stopPropagation();drag.current={y:e.clientY,height:e.currentTarget.parentElement?.offsetHeight||640};e.currentTarget.setPointerCapture(e.pointerId)}}
  onPointerMove={e=>{if(drag.current)apply(e.currentTarget,drag.current.height+(e.clientY-drag.current.y)*2)}}
  onPointerUp={e=>{if(drag.current){save(e.currentTarget);drag.current=null}}}
  onPointerCancel={e=>{if(drag.current){save(e.currentTarget);drag.current=null}}}
  onKeyDown={e=>{if(e.key==='ArrowUp'||e.key==='ArrowDown'){e.preventDefault();apply(e.currentTarget,(e.currentTarget.parentElement?.offsetHeight||640)+(e.key==='ArrowDown'?20:-20));save(e.currentTarget)}}}><span/></div>;
}
