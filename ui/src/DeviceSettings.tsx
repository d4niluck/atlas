import React from 'react';
import './rag-model-settings.css';
import {availableDevices, type Device} from './deviceOptions';
export type {Device} from './deviceOptions';
export type DeviceChoice = {device:Device; active_device:Device|null; restart_required:boolean; message:string};
export type DeviceStatus = {embedding:DeviceChoice;llm:DeviceChoice;startup_complete:boolean;restart_only:boolean;available_devices?:string[]};
const name=(device:Device)=>device==='metal'?'GPU Apple (Metal)':'CPU';
export function DeviceSettings({status,embedding,llm,onChange,which:only}:{which?:'embedding'|'llm';status:DeviceStatus|null;embedding:Device;llm:Device;onChange:(which:'embedding'|'llm',device:Device)=>void}) {
 return <section className="rag-settings" aria-label="Устройства для моделей"><div className="rag-settings-heading">Устройства для моделей</div>{status&&(['embedding','llm'] as const).filter(which=>!only||which===only).map(which=>{const choice=status[which],selected=which==='embedding'?embedding:llm,label=which==='embedding'?'Поиск по смыслу':'Модель ответа';return <div className="device-settings-group" key={which}><strong>{label}</strong><div className="rag-models" role="group" aria-label={'Устройство: '+label}>{availableDevices(status.available_devices).map(d=><button key={d} type="button" className="rag-model-option" aria-pressed={selected===d} onClick={()=>onChange(which,d)}><span>{name(d)}</span>{selected===d&&<span aria-hidden="true" className="rag-model-check">✓</span>}</button>)}</div><p role="status">Сейчас: {choice.active_device?name(choice.active_device):choice.message==='Модель занята'?'Модель занята':status.startup_complete?'Модель не загружена':'Подготовка модели'}</p>{choice.message&&<p>{choice.message}</p>}</div>})}</section>
}
