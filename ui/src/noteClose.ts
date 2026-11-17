// One in-flight save owns the editor snapshot until it completes.
export function createNoteClose(hide:(hidden:boolean)=>void, save:()=>Promise<boolean>, clear:()=>void){
 let pending:Promise<boolean>|null=null;
 return {
  wait:()=>pending??Promise.resolve(true),
  close:()=>{
   if(pending)return pending;
   hide(true);
   const task=Promise.resolve().then(save).then(ok=>{if(ok)clear();else hide(false);return ok},error=>{hide(false);throw error}).finally(()=>{pending=null});
   pending=task;return task;
  }
 };
}

export function editorBlocksBoard(edit:unknown, hidden:boolean){return Boolean(edit)&&!hidden}
