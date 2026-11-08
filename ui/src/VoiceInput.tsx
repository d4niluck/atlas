import React,{useEffect,useRef,useState} from 'react';
import {createVoiceSession} from './voiceSession';
import {encodeWav} from './voiceWav';
import './voice.css';
type Quality='base'|'small'|'medium';
const choices:readonly [Quality,string,string][]=[['base','Низкое','60 МБ'],['small','Среднее','190 МБ'],['medium','Высокое','539 МБ']];
const invoke=(name:string,args?:object):Promise<any> => (window as any).__TAURI__.core.invoke(name,args);
const selected=():Quality=>{const q=localStorage.getItem('atlas-voice-quality');return q==='base'||q==='medium'?q:'small'};

export function VoiceSettings(){
 const [quality,setQuality]=useState<Quality>(selected),[status,setStatus]=useState(''),[error,setError]=useState('');
 useEffect(()=>{let live=true;const refresh=()=>invoke('voice_model_status',{quality}).then(x=>{if(live)setStatus(x)}).catch(()=>{if(live)setStatus('missing')});void refresh();const timer=setInterval(refresh,1000);return()=>{live=false;clearInterval(timer)}},[quality]);
 async function download(){setStatus('downloading');setError('');try{await invoke('voice_download_model',{quality});setStatus('installed')}catch(e){setStatus('missing');setError(String(e))}}
 if(!/Mac/.test(navigator.platform))return null;
 return <section className="search-settings-block"><h3>Голосовой ввод</h3><div className="rag-models">{choices.map(([id,label,size])=><button type="button" key={id} className="rag-model-option" disabled={status==='downloading'} aria-pressed={quality===id} onClick={()=>{localStorage.setItem('atlas-voice-quality',id);setQuality(id);setError('')}}><span className="rag-model-copy"><strong>{label}</strong><small>{size}</small></span>{quality===id&&<span>✓</span>}</button>)}</div>{status==='installed'?<p>Модель загружена</p>:<button type="button" disabled={status==='downloading'} onClick={download}>{status==='downloading'?'Загрузка…':'Загрузить модель'}</button>}{error&&<p role="alert">{error}</p>}</section>
}

async function capture(stream:MediaStream){
 const context=new AudioContext(),input=context.createMediaStreamSource(stream),processor=context.createScriptProcessor(4096,1,1),chunks:Float32Array[]=[];let length=0;
 processor.onaudioprocess=e=>{if(length>=context.sampleRate*60)return;const data=e.inputBuffer.getChannelData(0);const remaining=Math.max(0,Math.min(data.length,context.sampleRate*60-length));chunks.push(data.slice(0,remaining));length+=remaining};
 const mute=context.createGain();mute.gain.value=0;input.connect(processor);processor.connect(mute);mute.connect(context.destination);
 const close=()=>{processor.disconnect();input.disconnect();mute.disconnect();processor.onaudioprocess=null;void context.close()};
 return {cancel:close,async stop(){close();const source=new Float32Array(length);let offset=0;for(const chunk of chunks){source.set(chunk,offset);offset+=chunk.length}return encodeWav(source,context.sampleRate)}};
}

export function VoiceInput({onText,active=true}:{onText:(text:string)=>void;active?:boolean}){
 const [state,setState]=useState<'idle'|'starting'|'recording'|'thinking'|'missing'|'downloading'>('idle'),[error,setError]=useState('');
 const quality=useRef<Quality>('small'),session=useRef<ReturnType<typeof createVoiceSession>|null>(null),generation=useRef(0);
 if(!session.current)session.current=createVoiceSession({getUserMedia:()=>navigator.mediaDevices.getUserMedia({audio:true}),capture,transcribe:(wav,id)=>invoke('voice_transcribe',{quality:quality.current,id,wav:Array.from(wav)}),cancel:id=>invoke('voice_cancel',{id})});
 useEffect(()=>()=>{void session.current?.cancel()},[]);
 useEffect(()=>{if(!active){generation.current++;void session.current?.cancel();setState('idle')}},[active]);
 useEffect(()=>{if(state!=='recording')return;const timer=setTimeout(()=>void stop(),60000);return()=>clearTimeout(timer)},[state]);
 useEffect(()=>{if(state!=='recording'&&state!=='thinking')return;const cancelOnBlur=()=>void cancel();window.addEventListener('blur',cancelOnBlur);return()=>window.removeEventListener('blur',cancelOnBlur)},[state]);
 async function start(){const n=++generation.current;setError('');setState('starting');try{quality.current=selected();const status=await invoke('voice_model_status',{quality:quality.current});if(n!==generation.current)return;if(status!=='installed'){setState('missing');return}await session.current!.start();if(n===generation.current)setState('recording')}catch(e){if(n===generation.current){setState('idle');setError('Не удалось включить микрофон: '+String(e))}}}
 async function stop(){const n=generation.current;setState('thinking');try{const text=await session.current!.stop();if(n===generation.current&&text)onText(text)}catch(e){if(n===generation.current)setError('Не удалось распознать речь: '+String(e))}finally{if(n===generation.current)setState('idle')}}
 async function cancel(){generation.current++;await session.current!.cancel().catch(()=>{});setState('idle')}
 async function download(){setState('downloading');try{await invoke('voice_download_model',{quality:quality.current});setState('idle')}catch(e){setState('missing');setError(String(e))}}
 if(!/Mac/.test(navigator.platform))return null;
 return <span className="voice-control"><button type="button" aria-label={state==='recording'?'Завершить запись':state==='thinking'||state==='starting'?'Отменить голосовой ввод':'Голосовой ввод'} disabled={state==='downloading'} onClick={()=>void(state==='recording'?stop():state==='thinking'||state==='starting'?cancel():start())}>{state==='recording'?'■':state==='thinking'||state==='starting'?'×':<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 11a7 7 0 0014 0M12 18v4m-4 0h8"/></svg>}</button>{state==='missing'&&<span className="voice-message">Нужна модель {choices.find(x=>x[0]===quality.current)?.[2]} <button type="button" onClick={()=>void download()}>Загрузить</button><button type="button" onClick={()=>setState('idle')}>×</button></span>}{state==='downloading'&&<span className="voice-message">Загрузка модели…</span>}{error&&<span className="voice-message" role="alert">{error}</span>}</span>
}
