import React, {useEffect, useState} from 'react';
import {AgenticToggle} from './AgenticToggle';
import {DeviceSettings, type Device, type DeviceStatus} from './DeviceSettings';
import {RagModelSettings} from './RagModelSettings';
import {SearchSettingsDialog} from './SearchSettingsDialog';
type Model='e2b'|'e4b';
export type SearchSettingsValue={agentic:boolean;model:Model;embedding:Device;llm:Device};
export function SearchSettings({onClose,onApply,installedE2b,installedE4b}:{onClose:()=>void;onApply:(value:SearchSettingsValue)=>Promise<void>;installedE2b:boolean;installedE4b:boolean}) {
 const [saved,setSaved]=useState<SearchSettingsValue|null>(null),[draft,setDraft]=useState<SearchSettingsValue|null>(null),[status,setStatus]=useState<DeviceStatus|null>(null),[error,setError]=useState(''),[confirming,setConfirming]=useState(false),[busy,setBusy]=useState(false);
 useEffect(()=>{const controller=new AbortController();let timer:ReturnType<typeof setTimeout>;async function refresh(){try{const r=await fetch('/api/settings/device',{signal:controller.signal});if(!r.ok)throw Error();const s=await r.json();if(!controller.signal.aborted)setStatus(s)}catch{}finally{if(!controller.signal.aborted)timer=setTimeout(refresh,5000)}}void refresh();fetch('/api/settings/app',{signal:controller.signal}).then(async r=>{if(!r.ok)throw Error();return r.json() as Promise<SearchSettingsValue>}).then(v=>{if(!controller.signal.aborted){setSaved(v);setDraft(v)}}).catch(()=>{if(!controller.signal.aborted)setError('Не удалось загрузить настройки.')});return()=>{controller.abort();clearTimeout(timer)}},[]);
 const dirty=!!saved&&!!draft&&(['agentic','model','embedding','llm'] as const).some(key=>saved[key]!==draft[key]);
 async function confirm(){if(!draft)return;setBusy(true);setError('');try{await onApply(draft)}catch(e){setConfirming(false);setError(e instanceof Error?e.message:'Не удалось применить настройки.')}finally{setBusy(false)}}
 return <SearchSettingsDialog onClose={onClose} dirty={dirty} onApply={()=>setConfirming(true)} busy={busy} confirming={confirming} onCancelConfirm={()=>setConfirming(false)} onConfirm={()=>void confirm()}>{draft&&<><AgenticToggle enabled={draft.agentic} disabled={false} onChange={agentic=>setDraft({...draft,agentic})}/><RagModelSettings model={draft.model} installedE2b={installedE2b} installedE4b={installedE4b} busy={false} onChange={model=>setDraft({...draft,model})}/><DeviceSettings status={status} embedding={draft.embedding} llm={draft.llm} onChange={(which,device)=>setDraft({...draft,[which]:device})}/></>}{error&&<p role="alert">{error}</p>}</SearchSettingsDialog>
}
