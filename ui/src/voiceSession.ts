type Capture={stop:()=>Promise<Uint8Array>;cancel:()=>void};
type VoiceSessionOptions={getUserMedia:()=>Promise<MediaStream>;capture:(stream:MediaStream)=>Promise<Capture>;transcribe:(wav:Uint8Array,id:string)=>Promise<string>;cancel:(id:string)=>Promise<void>;cancelPermission?:()=>Promise<void>};
export function createVoiceSession(options:VoiceSessionOptions){
 let generation=0, source:Capture|null=null, stream:MediaStream|null=null, activeId:string|null=null;
 const release=()=>{stream?.getTracks().forEach(t=>t.stop());stream=null};
 return {
  async start(){const n=++generation;const next=await options.getUserMedia();if(n!==generation){next.getTracks().forEach(t=>t.stop());return}stream=next;try{const capture=await options.capture(next);if(n!==generation){capture.cancel();return}source=capture}catch(e){release();throw e}},
  async stop(){const n=generation,current=source;source=null;if(!current)return null;let wav:Uint8Array;try{wav=await current.stop()}finally{release()}if(n!==generation)return null;const id=crypto.randomUUID();activeId=id;try{const text=await options.transcribe(wav,id);return n===generation?text:null}finally{if(activeId===id)activeId=null}},
  async cancel(){generation++;source?.cancel();source=null;release();const id=activeId;activeId=null;try{await options.cancelPermission?.()}finally{if(id)await options.cancel(id)}}
 };
}
