import {useEffect,useRef} from 'react';
import type {Dispatch, RefObject, SetStateAction} from 'react';

/** WebKit emits native gesture events for trackpad pinch, independently of wheel. */
export function usePinchZoom(ref:RefObject<HTMLElement|null>, enabled:boolean, zoom:number, setZoom:Dispatch<SetStateAction<number>>, min:number, max:number) {
 const currentZoom=useRef(zoom);currentZoom.current=zoom;
 useEffect(()=>{
  const element=ref.current;if(!element||!enabled)return;
  let baseline=1;
  const start=(event:Event)=>{event.preventDefault();baseline=currentZoom.current};
  const change=(event:Event)=>{event.preventDefault();const scale=(event as Event & {scale?:number}).scale;if(typeof scale==='number'&&Number.isFinite(scale)&&scale>0)setZoom(Math.max(min,Math.min(max,baseline*scale)))};
  element.addEventListener('gesturestart',start,{passive:false});element.addEventListener('gesturechange',change,{passive:false});
  return()=>{element.removeEventListener('gesturestart',start);element.removeEventListener('gesturechange',change)};
 },[ref,enabled,setZoom,min,max]);
}
