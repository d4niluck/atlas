type Notice={id:string;prefix:string};
type NativeEvents={listen:(name:string,callback:(event:{payload:unknown})=>void)=>Promise<()=>void>;emit:(name:string,payload:Notice)=>Promise<void>};
type SyncWindow=Window & {__TAURI__?:{event?:NativeEvents}};
const name='atlas-chat-cleared';

// The native event reaches both webviews; storage and a DOM event cover browser mode.
export function subscribeChatClear(win:SyncWindow,prefix:string,onClear:()=>void):()=>void {
 const seen=new Set<string>();let stopped=false,unlisten:(()=>void)|undefined;
 const receive=(value:unknown)=>{if(!value||typeof value!=='object')return;const notice=value as Partial<Notice>;if(notice.prefix!==prefix||typeof notice.id!=='string'||seen.has(notice.id))return;seen.add(notice.id);if(seen.size>32)seen.delete(seen.values().next().value!);onClear()};
 const local=(event:Event)=>receive((event as CustomEvent).detail);
 const storage=(event:Event)=>{const e=event as StorageEvent;if(e.key!==name||!e.newValue)return;try{receive(JSON.parse(e.newValue))}catch{}};
 win.addEventListener(name,local);win.addEventListener('storage',storage);
 const events=win.__TAURI__?.event;
 if(events)void events.listen(name,event=>receive(event.payload)).then(off=>{if(stopped)off();else unlisten=off}).catch(()=>{});
 return ()=>{stopped=true;unlisten?.();win.removeEventListener(name,local);win.removeEventListener('storage',storage)};
}
export async function publishChatClear(win:SyncWindow,prefix:string):Promise<void> {
 const notice={id:crypto.randomUUID(),prefix};
 win.dispatchEvent(new CustomEvent(name,{detail:notice}));
 try{win.localStorage.setItem(name,JSON.stringify(notice))}catch{}
 try{await win.__TAURI__?.event?.emit(name,notice)}catch{}
}
