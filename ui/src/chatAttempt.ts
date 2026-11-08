export type ChatAttempt = {id:string;request:AbortController;poll:AbortController;scopeChanged:boolean};

/** Stop showing an old scope immediately, before awaiting the server. */
export function cancelForScopeChange(attempt:ChatAttempt,cancel:()=>Promise<void>):Promise<void> {
 attempt.scopeChanged=true;
 attempt.request.abort();
 attempt.poll.abort();
 return cancel();
}

export function acceptsChatEvents(active:ChatAttempt|null,attempt:ChatAttempt):boolean {
 return active===attempt&&!attempt.request.signal.aborted;
}
