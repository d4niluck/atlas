import React, {useEffect, useRef, useState} from 'react';
import {clampSearchPanelWidth, SEARCH_PANEL_DEFAULT, searchPanelWidthFromPointer} from './searchPanelSizing';

const STORAGE_KEY='atlas-search-panel-width';
export function useSearchPanelWidth() {
  const panelRef=useRef<HTMLElement>(null);
  const [width,setWidth]=useState(()=>{
    const raw=localStorage.getItem(STORAGE_KEY);
    return clampSearchPanelWidth(raw===null ? SEARCH_PANEL_DEFAULT : Number(raw),window.innerWidth);
  });
  const drag=useRef(false);
  const right=useRef(0);
  const setAndSave=(next:number)=>setWidth(()=>{
    const w=clampSearchPanelWidth(next,window.innerWidth);
    localStorage.setItem(STORAGE_KEY,String(w));
    return w;
  });
  useEffect(()=>{
    const resize=()=>setWidth(w=>clampSearchPanelWidth(w,window.innerWidth));
    window.addEventListener('resize',resize);
    return()=>window.removeEventListener('resize',resize);
  },[]);
  const handle=<div role="separator" tabIndex={0} aria-label="Изменить ширину поиска" aria-orientation="vertical"
    aria-valuemin={Math.min(280,Math.max(0,window.innerWidth-40))} aria-valuemax={Math.max(0,window.innerWidth-40)} aria-valuenow={Math.round(width)}
    className="search-panel-resize" onPointerDown={e=>{if(e.button!==0)return;e.preventDefault();drag.current=true;right.current=panelRef.current?.getBoundingClientRect().right??window.innerWidth;e.currentTarget.setPointerCapture(e.pointerId)}}
    onPointerMove={e=>{if(drag.current)setAndSave(searchPanelWidthFromPointer(right.current,e.clientX,window.innerWidth))}}
    onPointerUp={e=>{drag.current=false;if(e.currentTarget.hasPointerCapture(e.pointerId))e.currentTarget.releasePointerCapture(e.pointerId)}}
    onPointerCancel={()=>{drag.current=false}}
    onKeyDown={e=>{let target:number|undefined;if(e.key==='ArrowLeft')target=width+20;else if(e.key==='ArrowRight')target=width-20;else if(e.key==='Home')target=280;else if(e.key==='End')target=window.innerWidth-40;if(target!==undefined){e.preventDefault();setAndSave(target)}}} />;
  return {panelRef,width,handle};
}
