import type {SearchChunk} from './SearchResults';

export type MenuTarget = {workspaceId:string; id:string; chunk:SearchChunk|null};

// A request can arrive before the workspace is mounted or before opening it finishes.
export function createMenuNavigation({openWorkspace}:{openWorkspace:(id:string)=>Promise<boolean>}) {
 let pending:MenuTarget|null=null;
 let generation=0;
 let accepted=false;
 let callback:((target:MenuTarget)=>void)|null=null;
 const deliver=()=>{if(pending&&accepted&&callback){const target=pending;const open=callback;pending=null;callback=null;accepted=false;open(target)}};
 return {
  async request(target:MenuTarget){
   const current=++generation;pending=target;accepted=false;callback=null;
   try {const opened=await openWorkspace(target.workspaceId);if(current!==generation)return;if(!opened){pending=null;callback=null;return}accepted=true;deliver()}
   catch(error){if(current===generation){pending=null;callback=null}throw error}
  },
  ready(workspaceId:string, openSource:(target:MenuTarget)=>void){
   if(!pending||pending.workspaceId!==workspaceId)return;
   callback=openSource;deliver();
  }
 };
}
