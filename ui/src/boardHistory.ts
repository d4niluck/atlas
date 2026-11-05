// Undo board layout without replacing current note/file metadata.
type Position = {id:string; x:number; y:number; imageExpanded?:boolean};
type Layout = {objects:Position[]; edges:unknown[]; stacks:unknown[]};
type Entry = {before:Layout; after:Layout; kind:'layout'} | {before:Layout; after:Layout; kind:'created'; id:string; revision:string} | {before:Layout; after:Layout; kind:'deleted'; id:string} | {before:Layout; after:Layout; kind:'deleted-group'; ids:string[]};
export type UndoPlan = {kind:'layout'; board:Layout} | {kind:'created'; id:string; revision:string} | {kind:'deleted'; id:string} | {kind:'deleted-group'; ids:string[]};
function snapshot(board:Layout):Layout {
 return {objects:board.objects.map(({id,x,y,imageExpanded})=>({id,x,y,imageExpanded:!!imageExpanded})),edges:structuredClone(board.edges),stacks:structuredClone(board.stacks)};
}
function same(a:Layout,b:Layout){return JSON.stringify(snapshot(a))===JSON.stringify(snapshot(b));}
function ids(x:Layout){return JSON.stringify(x.objects.map(o=>o.id).sort());}
export class BoardHistory {
 private entries:Entry[]=[];
 private push(entry:Entry){this.entries.push(entry);if(this.entries.length>100)this.entries.shift();}
 record(before:Layout, after:Layout) {
  if(ids(before)!==ids(after)){this.entries=[];return}
  if(same(before,after))return;
  this.push({kind:'layout',before:snapshot(before),after:snapshot(after)});
 }
 recordCreated(before:Layout,after:Layout,id:string,revision:string){
  if(!revision || before.objects.some(o=>o.id===id) || !after.objects.some(o=>o.id===id) ||
     ids(before)!==ids({...after,objects:after.objects.filter(o=>o.id!==id)})){this.clear();return}
  this.push({kind:'created',id,revision,before:snapshot(before),after:snapshot(after)});
 }
 recordDeleted(before:Layout,after:Layout,id:string){
  if(!before.objects.some(o=>o.id===id) || after.objects.some(o=>o.id===id) ||
     ids({...before,objects:before.objects.filter(o=>o.id!==id)})!==ids(after)){this.clear();return}
  this.push({kind:'deleted',id,before:snapshot(before),after:snapshot(after)});
 }
 recordDeletedGroup(before:Layout,after:Layout,removed:string[]){
  const removedSet=new Set(removed);
  if(!removed.length || removedSet.size!==removed.length || removed.some(id=>!before.objects.some(o=>o.id===id) || after.objects.some(o=>o.id===id)) ||
     ids({...before,objects:before.objects.filter(o=>!removedSet.has(o.id))})!==ids(after)){this.clear();return}
  this.push({kind:'deleted-group',ids:[...removed],before:snapshot(before),after:snapshot(after)});
 }
 clear(){this.entries=[]}
 prepareUndo<T extends Layout>(current:T):UndoPlan|null{
  const entry=this.entries.at(-1);
  if(!entry || !same(entry.after,current))return null;
  if(entry.kind==='created')return {kind:'created',id:entry.id,revision:entry.revision};
  if(entry.kind==='deleted')return {kind:'deleted',id:entry.id};
  if(entry.kind==='deleted-group')return {kind:'deleted-group',ids:[...entry.ids]};
  const positions=new Map(entry.before.objects.map(o=>[o.id,o]));
  return {kind:'layout',board:{...current,objects:current.objects.map(o=>({...o,...positions.get(o.id)})),
   edges:structuredClone(entry.before.edges),stacks:structuredClone(entry.before.stacks)}};
 }
 confirmUndo(){this.entries.pop()}
 undo<T extends Layout>(current:T):T|null {
  const plan=this.prepareUndo(current);
  if(!plan || plan.kind!=='layout')return null;
  this.confirmUndo();return plan.board as T;
 }
}
