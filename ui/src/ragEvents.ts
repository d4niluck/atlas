import type {SearchResult} from './SearchResults';

type RagEvent = {seq:number;type:'progress';stage:string;text:string} | {seq:number;type:'results';results:SearchResult[]} | {seq:number;type:'token';text:string} | {seq:number;type:'reset'} | {seq:number;type:'done'} | {seq:number;type:'error';error:string};

/** Polling keeps streaming working through the native HTTP proxy on macOS and Linux. */
export async function watchRagEvents(prefix:string,id:string,signal:AbortSignal,onEvent:(event:RagEvent)=>void,fetcher:typeof fetch=fetch,interval=200):Promise<void> {
 let after=0;
 while(!signal.aborted){
  const response=await fetcher(`${prefix}/rag/events/${encodeURIComponent(id)}?after=${after}`,{signal});
  if(response.status!==404){
   if(!response.ok)throw Error('Не удалось получить ход ответа');
   const data=await response.json() as {events:RagEvent[];done:boolean};
   for(const event of data.events){if(signal.aborted)return;if(!Number.isSafeInteger(event.seq)||event.seq<=after)continue;after=event.seq;onEvent(event)}
   if(data.done)return;
  }
  await new Promise<void>((resolve,reject)=>{
   if(signal.aborted){reject(new DOMException('Aborted','AbortError'));return}
   const timer=setTimeout(()=>{signal.removeEventListener('abort',abort);resolve()},interval);
   function abort(){clearTimeout(timer);reject(new DOMException('Aborted','AbortError'))}
   signal.addEventListener('abort',abort,{once:true});
  });
 }
}
