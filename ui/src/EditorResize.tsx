import {useCallback, useRef} from 'react';

export function useEditorResize(expanded:boolean){
 const cleanup=useRef<(()=>void)|null>(null);
 return useCallback((element:HTMLElement|null)=>{
  cleanup.current?.(); cleanup.current=null;
  if(!element || expanded)return;
  let drag:{x:number;y:number;width:number;height:number;horizontal:number;vertical:number}|null=null;
  function edges(e:PointerEvent){const r=element!.getBoundingClientRect(),gap=9;return {horizontal:e.clientX-r.left<gap?-1:r.right-e.clientX<gap?1:0,vertical:e.clientY-r.top<gap?-1:r.bottom-e.clientY<gap?1:0};}
  function down(e:PointerEvent){if(e.button!==0)return;const edge=edges(e);if(!edge.horizontal&&!edge.vertical)return;e.preventDefault();e.stopPropagation();drag={x:e.clientX,y:e.clientY,width:element!.offsetWidth,height:element!.offsetHeight,...edge};element!.setPointerCapture(e.pointerId);}
  function move(e:PointerEvent){if(drag){if(drag.horizontal)element!.style.width=`${Math.round(Math.max(320,Math.min(innerWidth*.95,drag.width+2*drag.horizontal*(e.clientX-drag.x))))}px`;if(drag.vertical)element!.style.height=`${Math.round(Math.max(300,Math.min(innerHeight*.85,drag.height+2*drag.vertical*(e.clientY-drag.y))))}px`;return}const edge=edges(e);element!.style.cursor=edge.horizontal&&edge.vertical?(edge.horizontal===edge.vertical?'nwse-resize':'nesw-resize'):edge.horizontal?'ew-resize':edge.vertical?'ns-resize':'';}
  function up(){if(!drag)return;localStorage.setItem('atlas-editor-height',String(element!.offsetHeight));localStorage.setItem('atlas-editor-width',String(element!.offsetWidth));drag=null;}
  function keyboard(e:KeyboardEvent){if(e.target!==element || !e.altKey || !e.shiftKey)return;const key=e.key;if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(key))return;e.preventDefault();if(key==='ArrowLeft'||key==='ArrowRight')element!.style.width=`${Math.round(Math.max(320,Math.min(innerWidth*.95,element!.offsetWidth+(key==='ArrowRight'?20:-20))))}px`;else element!.style.height=`${Math.round(Math.max(300,Math.min(innerHeight*.85,element!.offsetHeight+(key==='ArrowDown'?20:-20))))}px`;localStorage.setItem('atlas-editor-height',String(element!.offsetHeight));localStorage.setItem('atlas-editor-width',String(element!.offsetWidth));}
  element.addEventListener('keydown',keyboard);element.addEventListener('pointerdown',down);element.addEventListener('pointermove',move);element.addEventListener('pointerup',up);element.addEventListener('pointercancel',up);
  cleanup.current=()=>{element.removeEventListener('keydown',keyboard);element.removeEventListener('pointerdown',down);element.removeEventListener('pointermove',move);element.removeEventListener('pointerup',up);element.removeEventListener('pointercancel',up);};
 },[expanded]);
}
