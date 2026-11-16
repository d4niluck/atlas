// Register the next flush before yielding so every waiting caller rechecks the owner.
export async function acquireBoardFlush(ref:{current:Promise<void>|null}):Promise<()=>void>{
 while(ref.current)await ref.current;
 let release!:()=>void;
 const barrier=new Promise<void>(resolve=>{release=resolve});
 ref.current=barrier;
 return ()=>{if(ref.current===barrier)ref.current=null;release()};
}

export function canApplyRemoteBoard(s:{startedRevision:string|null;currentRevision:string|null;remoteRevision:string;dirty:boolean;pending:boolean;busy:boolean;dragging:boolean;conflict:boolean}):boolean{
 return !!s.startedRevision&&s.startedRevision===s.currentRevision&&s.remoteRevision!==s.currentRevision&&!s.dirty&&!s.pending&&!s.busy&&!s.dragging&&!s.conflict;
}

export function shouldFetchRemoteBoard(previous:number|null,current:number):boolean{return previous===null||previous!==current}

export function remotePreviewAction(id:string|null,ids:string[],descriptionDirty:boolean):"keep"|"defer"|"close"{return id&&!ids.includes(id)?descriptionDirty?"defer":"close":"keep"}
