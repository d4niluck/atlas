import React,{useEffect,useRef,useState} from 'react';
import {createVoiceSession} from './voiceSession';
import {encodeWav} from './voiceWav';
import './voice.css';
type Quality='base'|'small'|'medium';
const choices:readonly [Quality,string,string][]=[['base','Низкое','60 МБ'],['small','Среднее','190 МБ'],['medium','Высокое','539 МБ']];
const invoke=(name:string,args?:object):Promise<any> => (window as any).__TAURI__.core.invoke(name,args);
async function selected():Promise<Quality>{const q=await invoke('voice_get_quality');if(q==='base'||q==='small'||q==='medium')return q;throw Error('Неизвестная модель распознавания')}

export function VoiceSettings(){
 const [quality,setQuality]=useState<Quality|null>(null),[status,setStatus]=useState(''),[error,setError]=useState(''),[saving,setSaving]=useState(false);const localDownload=useRef(false);
 useEffect(()=>{let live=true;selected().then(q=>{if(live)setQuality(q)}).catch(e=>{if(live)setError(String(e))});return()=>{live=false}},[]);
 useEffect(()=>{if(!quality)return;let live=true;const refresh=()=>{if(localDownload.current)return;return invoke('voice_model_status',{quality}).then(x=>{if(live)setStatus(x)}).catch(()=>{if(live)setStatus('missing')})};void refresh();const timer=status==='downloading'?setInterval(refresh,3000):undefined;return()=>{live=false;if(timer)clearInterval(timer)}},[quality,status==='downloading']);
 async function choose(id:Quality){setSaving(true);setError('');try{await invoke('voice_set_quality',{quality:id});setQuality(id);setStatus('')}catch(e){setError('Не удалось сохранить настройку: '+String(e))}finally{setSaving(false)}}
 async function download(){const repair=status==='corrupt';localDownload.current=true;setStatus('downloading');setError('');try{if(repair)await invoke('voice_repair_model',{quality});await invoke('voice_download_model',{quality});setStatus('installed')}catch(e){setStatus(repair?'corrupt':'missing');setError(String(e))}finally{localDownload.current=false}}
 if(!/Mac/.test(navigator.platform))return null;
 return <section className="search-settings-block"><h3>Голосовой ввод</h3><div className="rag-models">{choices.map(([id,label,size])=><button type="button" key={id} className="rag-model-option" disabled={!quality||saving||status==='downloading'} aria-pressed={quality===id} onClick={()=>void choose(id)}><span className="rag-model-copy"><strong>{label}</strong><small>{size}</small></span>{quality===id&&<span>✓</span>}</button>)}</div>{status==='installed'?<p>Модель загружена</p>:<button type="button" disabled={!quality||saving||status==='downloading'} onClick={download}>{status==='downloading'?'Загрузка…':status==='corrupt'?'Переустановить модель':'Загрузить модель'}</button>}{status==='corrupt'&&<p role="alert">Файл модели повреждён. Переустановите его.</p>}{error&&<p role="alert">{error}</p>}</section>
}

async function capture(stream:MediaStream){
 const context=new AudioContext(),input=context.createMediaStreamSource(stream),processor=context.createScriptProcessor(4096,1,1),chunks:Float32Array[]=[];let length=0;
 processor.onaudioprocess=e=>{if(length>=context.sampleRate*60)return;const data=e.inputBuffer.getChannelData(0);const remaining=Math.max(0,Math.min(data.length,context.sampleRate*60-length));chunks.push(data.slice(0,remaining));length+=remaining};
 const mute=context.createGain();mute.gain.value=0;input.connect(processor);processor.connect(mute);mute.connect(context.destination);
 const close=()=>{processor.disconnect();input.disconnect();mute.disconnect();processor.onaudioprocess=null;void context.close()};
 return {cancel:close,async stop(){close();const source=new Float32Array(length);let offset=0;for(const chunk of chunks){source.set(chunk,offset);offset+=chunk.length}return encodeWav(source,context.sampleRate)}};
}

export function VoiceInput({onText,active=true}:{onText:(text:string)=>void;active?:boolean}){
 const [state,setState]=useState<'idle'|'starting'|'recording'|'thinking'|'missing'|'corrupt'|'downloading'>('idle'),[error,setError]=useState('');
 const quality=useRef<Quality>('small'),session=useRef<ReturnType<typeof createVoiceSession>|null>(null),generation=useRef(0);
 if(!session.current)session.current=createVoiceSession({getUserMedia:()=>navigator.mediaDevices.getUserMedia({audio:true}),capture,transcribe:(wav,id)=>invoke('voice_transcribe',{quality:quality.current,id,wav:Array.from(wav)}),cancel:id=>invoke('voice_cancel',{id})});
 useEffect(()=>()=>{void session.current?.cancel()},[]);
 useEffect(()=>{if(!active){generation.current++;void session.current?.cancel();setState('idle')}},[active]);
 useEffect(()=>{if(state!=='recording')return;const timer=setTimeout(()=>void stop(),60000);return()=>clearTimeout(timer)},[state]);
 useEffect(()=>{if(state!=='recording'&&state!=='thinking')return;const cancelOnBlur=()=>void cancel();window.addEventListener('blur',cancelOnBlur);return()=>window.removeEventListener('blur',cancelOnBlur)},[state]);
 useEffect(()=>{if(state!=='starting'&&state!=='recording'&&state!=='thinking')return;const onVisibility=()=>{if(document.hidden)void cancel()};document.addEventListener('visibilitychange',onVisibility);return()=>document.removeEventListener('visibilitychange',onVisibility)},[state]);
 async function start(){const n=++generation.current;setError('');setState('starting');try{quality.current=await selected();if(n!==generation.current)return;const status=await invoke('voice_model_status',{quality:quality.current});if(n!==generation.current)return;if(status!=='installed'){setState(status==='corrupt'?'corrupt':'missing');return}await session.current!.start();if(n===generation.current)setState('recording')}catch(e){if(n===generation.current){setState('idle');setError('Не удалось включить микрофон: '+String(e))}}}
 async function stop(){const n=generation.current;setState('thinking');try{const text=await session.current!.stop();if(n===generation.current&&text)onText(text)}catch(e){if(n===generation.current)setError('Не удалось распознать речь: '+String(e))}finally{if(n===generation.current)setState('idle')}}
 async function cancel(){generation.current++;await session.current!.cancel().catch(()=>{});setState('idle')}
 async function download(){const repair=state==='corrupt';setState('downloading');try{if(repair)await invoke('voice_repair_model',{quality:quality.current});await invoke('voice_download_model',{quality:quality.current});setState('idle')}catch(e){setState(repair?'corrupt':'missing');setError(String(e))}}
 if(!/Mac/.test(navigator.platform))return null;
 return <span className="voice-control"><button type="button" aria-label={state==='recording'?'Завершить запись':state==='thinking'||state==='starting'?'Отменить голосовой ввод':'Голосовой ввод'} disabled={state==='downloading'} onClick={()=>void(state==='recording'?stop():state==='thinking'||state==='starting'?cancel():start())}>{state==='recording'?'■':state==='thinking'||state==='starting'?'×':<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 11a7 7 0 0014 0M12 18v4m-4 0h8"/></svg>}</button>{(state==='missing'||state==='corrupt')&&<span className="voice-message">{state==='corrupt'?'Модель повреждена. Переустановите':'Нужна модель'} {choices.find(x=>x[0]===quality.current)?.[2]} <button type="button" onClick={()=>void download()}>{state==='corrupt'?'Переустановить':'Загрузить'}</button><button type="button" onClick={()=>setState('idle')}>×</button></span>}{state==='downloading'&&<span className="voice-message">Загрузка модели…</span>}{error&&<span className="voice-message" role="alert">{error}</span>}</span>
}
