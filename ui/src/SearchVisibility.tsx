import React, {useEffect,useState} from 'react';
import './search-visibility.css';

export function SearchVisibility({prefix,id,onError}:{prefix:string;id:string;onError:(message:string)=>void}) {
 const [excluded,setExcluded]=useState<boolean|null>(null);
 const [busy,setBusy]=useState(false);
 const url=prefix+'/objects/'+encodeURIComponent(id)+'/search-visibility';
 useEffect(()=>{let active=true;setExcluded(null);fetch(url).then(async response=>{if(!response.ok)throw Error('Не удалось узнать состояние поиска');return response.json() as Promise<{excluded:boolean}>}).then(data=>{if(active)setExcluded(data.excluded)}).catch(e=>{if(active)onError(String(e))});return()=>{active=false}},[url]);
 const label=excluded?'Вернуть в общий поиск':'Исключить из общего поиска';
 const toggle=async()=>{if(excluded===null||busy)return;setBusy(true);try{const response=await fetch(url,{method:'PUT',headers:{'Content-Type':'application/json'},body:JSON.stringify({excluded:!excluded})});if(!response.ok)throw Error('Не удалось изменить участие в поиске');const data=await response.json() as {excluded:boolean};setExcluded(data.excluded)}catch(e){onError(String(e))}finally{setBusy(false)}};
 return <button type="button" className="search-visibility" aria-label={label} title={label} aria-pressed={excluded===true} disabled={excluded===null||busy} onClick={()=>void toggle()}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="10" cy="10" r="6"/><path d="m14.5 14.5 5 5"/>{excluded&&<path d="M4 20 20 4"/>}</svg></button>;
}
