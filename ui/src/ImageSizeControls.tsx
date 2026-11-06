import React,{useEffect,useRef} from 'react';
import {imageCardWidth,stepImageCardWidth,MIN_IMAGE_CARD_WIDTH,MAX_IMAGE_CARD_WIDTH} from './imageCardSize';

type Props={width?:number|null;onChange:(width:number)=>void;onGeometryChange:()=>void};
export function ImageSizeControls({width,onChange,onGeometryChange}:Props){
 const control=useRef<HTMLSpanElement>(null);
 const geometryCallback=useRef(onGeometryChange);geometryCallback.current=onGeometryChange;
 useEffect(()=>{
  const card=control.current?.closest<HTMLElement>('[data-card-id]');
  if(!card||typeof ResizeObserver==='undefined')return;
  let previous='';
  const observer=new ResizeObserver(()=>{const current=`${card.offsetWidth}:${card.offsetHeight}`;if(current!==previous){previous=current;geometryCallback.current()}});
  observer.observe(card);return()=>observer.disconnect();
 },[]);
 const current=imageCardWidth(width);
 return <span ref={control} className="image-size-controls" onPointerDown={event=>event.stopPropagation()} onPointerUp={event=>event.stopPropagation()} onClick={event=>event.stopPropagation()} onDoubleClick={event=>event.stopPropagation()} onKeyDown={event=>event.stopPropagation()}>
  <button type="button" aria-label="Уменьшить изображение на доске" title="Уменьшить изображение" disabled={current<=MIN_IMAGE_CARD_WIDTH} onClick={()=>onChange(stepImageCardWidth(current,-1))}>−</button>
  <button type="button" aria-label="Увеличить изображение на доске" title="Увеличить изображение" disabled={current>=MAX_IMAGE_CARD_WIDTH} onClick={()=>onChange(stepImageCardWidth(current,1))}>＋</button>
 </span>;
}
