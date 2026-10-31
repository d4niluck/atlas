import React, {forwardRef, useCallback, useEffect, useImperativeHandle, useRef, useState} from 'react';
import {SearchExcerpt} from './SearchExcerpt';
import './file-description.css';

export type FileDescriptionHandle = {flush: () => Promise<void>};
type Props = {prefix:string; id:string; suggestDescription?:boolean; highlight?:{start:number|null;end:number|null}|null; onDirty:(dirty:boolean)=>void; onError:(message:string)=>void};

export const FileDescription = forwardRef<FileDescriptionHandle,Props>(function FileDescription({prefix,id,suggestDescription=false,highlight,onDirty,onError}, ref) {
  const [text,setText]=useState('');
  const [loaded,setLoaded]=useState(false);
  const [saving,setSaving]=useState(false);
  const [open,setOpen]=useState(false);
  const value=useRef('');
  const version=useRef(0);
  const saved=useRef(0);
  const timer=useRef<ReturnType<typeof setTimeout>|null>(null);
  const queue=useRef<Promise<void>>(Promise.resolve());
  const url=prefix+'/objects/'+encodeURIComponent(id)+'/description';
  useEffect(()=>{let active=true;setLoaded(false);fetch(url).then(r=>{if(!r.ok)throw Error('Не удалось открыть описание');return r.json()}).then(data=>{if(active){value.current=data.description;setText(data.description);if(suggestDescription&&!data.description.trim())setOpen(true);version.current=0;saved.current=0;setLoaded(true)}}).catch(e=>{if(active)onError(String(e))});return()=>{active=false;if(timer.current)clearTimeout(timer.current)}},[url]);
  useEffect(()=>{if(highlight?.start!=null&&highlight.end!=null)setOpen(true)},[highlight?.start,highlight?.end]);
  const flush=useCallback(async()=>{
    if(timer.current){clearTimeout(timer.current);timer.current=null}
    const target=version.current, snapshot=value.current;
    if(target<=saved.current){await queue.current;return}
    const task=queue.current.catch(()=>{}).then(async()=>{
      setSaving(true);
      try {const response=await fetch(url,{method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify({description:snapshot})});if(!response.ok)throw Error('Не удалось сохранить описание');saved.current=Math.max(saved.current,target);if(version.current===target)onDirty(false)} finally {setSaving(false)}
    });
    queue.current=task;
    await task;
  },[url,onDirty]);
  useImperativeHandle(ref,()=>({flush}),[flush]);
  const update=(next:string)=>{value.current=next;setText(next);version.current++;onDirty(true);if(timer.current)clearTimeout(timer.current);timer.current=setTimeout(()=>{void flush().catch(e=>onError(String(e)))},1200)};
  return <details className="file-description" open={open} onToggle={e=>setOpen(e.currentTarget.open)}>
    <summary><span>Описание для поиска</span><small>{saving?'Сохранение…':!loaded?'Загрузка…':text.trim()?'Добавлено':'Не добавлено'}</small></summary>
    {highlight?.start!=null&&highlight.end!=null&&loaded?<SearchExcerpt text={text} start={highlight.start} end={highlight.end}/>:<textarea aria-label="Описание для поиска" value={text} disabled={!loaded} onChange={e=>update(e.target.value)} placeholder="Добавьте описание файла"/>}
  </details>;
});
