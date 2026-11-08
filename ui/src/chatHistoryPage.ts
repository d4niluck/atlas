export type ChatHistoryItem = {seq:number;role:'user'|'assistant';content:string};
export type ChatHistoryPage<T extends ChatHistoryItem> = {messages:T[];next_before:number|null};

export function parseChatHistoryPage<T extends ChatHistoryItem>(value:unknown):ChatHistoryPage<T> {
 if(!value||typeof value!=='object')throw Error('История чата недоступна');
 const page=value as {messages?:unknown;next_before?:unknown};
 const validSources=(items:unknown)=>items===undefined||Array.isArray(items)&&items.every((source:unknown)=>!!source&&typeof source==='object'&&typeof (source as {id?:unknown}).id==='string'&&typeof (source as {title?:unknown}).title==='string'&&typeof (source as {text?:unknown}).text==='string');
 if(!Array.isArray(page.messages)||!page.messages.every(m=>m&&Number.isSafeInteger(m.seq)&&m.seq>0&&['user','assistant'].includes(m.role)&&typeof m.content==='string'&&(m.scope===undefined||m.scope===null||Array.isArray(m.scope)&&m.scope.every((id:unknown)=>typeof id==='string'))&&(m.steps===undefined||Array.isArray(m.steps)&&m.steps.every((step:unknown)=>typeof step==='string'))&&validSources(m.sources)&&validSources(m.found_sources))||!(page.next_before===null||Number.isSafeInteger(page.next_before)&&Number(page.next_before)>0))throw Error('История чата недоступна');
 return page as ChatHistoryPage<T>;
}

export function mergeChatHistory<T extends ChatHistoryItem>(current:T[],incoming:T[]):T[] {
 const bySeq=new Map<number,T>();
 for(const message of current)bySeq.set(message.seq,message);
 for(const message of incoming)if(!bySeq.has(message.seq))bySeq.set(message.seq,message);
 return Array.from(bySeq.values()).sort((a,b)=>a.seq-b.seq);
}
