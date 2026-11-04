/** The native side must keep the window open until this preparation finishes. */
export async function prepareWindowExit(options:{
 begin:()=>Promise<unknown>;
 check:()=>void;
 lock:()=>void;
 prepare:()=>Promise<void>;
 cancel:()=>void;
 finish:(error:string|null)=>Promise<unknown>;
 notify:(message:string)=>void;
}):Promise<void>{
 try{await options.begin()}catch{options.notify('Не удалось подготовить закрытие Atlas.');return}
 try{options.check();options.lock();await options.prepare()}
 catch(error){const message=error instanceof Error?error.message:'Не удалось сохранить изменения.';options.cancel();try{await options.finish(message)}catch{options.notify(message)}return}
 // An invocation failure may mean the native side accepted the request but lost its reply.
 try{await options.finish(null)}catch{options.notify('Закрытие не подтверждено. Дождитесь завершения операции.')}
}
