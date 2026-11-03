import {mergeDownloadStatus} from './modelDownloadStatus';
import React,{useEffect,useState} from 'react';
type TauriWindow = Window & {__TAURI__?: {core: {invoke: (command:string) => Promise<unknown>}}};
export function ModelDownload({kind='search-q8'}:{kind?:'search'|'search-q8'|'answer'|'answer-e4b'|'vision'}){
 const [status,setStatus]=useState('checking');
 const invoke=(command:string)=>{const tauri=(window as TauriWindow).__TAURI__;return tauri?tauri.core.invoke(command):Promise.reject(new Error('Загрузка доступна в приложении Atlas'));};
 useEffect(()=>{let active=true;const refresh=()=>invoke(kind==='search-q8'?'q8_model_download_status':kind==='vision'?'vision_model_download_status':kind==='answer-e4b'?'rag_e4b_model_download_status':kind==='answer'?'rag_model_download_status':'model_download_status').then(s=>{if(active)setStatus(current=>mergeDownloadStatus(current,String(s)))}).catch(()=>{});refresh();const timer=setInterval(refresh,1000);return()=>{active=false;clearInterval(timer)}},[]);
 const download=async()=>{try{await invoke(kind==='search-q8'?'download_q8_model':kind==='vision'?'download_vision_model':kind==='answer-e4b'?'download_rag_e4b_model':kind==='answer'?'download_rag_model':'download_model');setStatus('downloading:0')}catch(e){setStatus('error:'+String(e))}};
 if(status==='checking'||status==='installed')return null;
 if(status==='complete')return <p>Модель загружена.</p>;
 if(status.startsWith('downloading:'))return <p>Загрузка модели: {status.slice(12)}%</p>;
 return <div>{status.startsWith('error:')&&<p role="alert">{status.slice(6)}</p>}<button className="model-download-button" type="button" onClick={()=>void download()}>Загрузить модель {kind==='vision'?'изображений':kind==='answer-e4b'?'Gemma E4B':kind==='answer'?'ответа':'поиска'}</button></div>;
}
