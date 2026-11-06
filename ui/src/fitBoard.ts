import type {BoardObject} from './selection';
type Stack={id:string;ids:string[];collapsed?:boolean};

export function stackFrame(members:BoardObject[],collapsed:boolean){
 if(!members.length)return null;
 const x=Math.min(...members.map(o=>o.x))-20,y=Math.min(...members.map(o=>o.y))-45;
 return {x,y,w:collapsed?240:Math.max(...members.map(o=>o.x+(o.width??230)))+20-x,
  h:collapsed?72:Math.max(...members.map(o=>o.y+(o.height??82)))+18-y};
}

export function extractionPosition(objects:BoardObject[],stacks:Stack[],id:string){
 const group=stacks.find(stack=>stack.ids.includes(id));
 const extracted=objects.find(item=>item.id===id);
 if(!group||!extracted)return null;
 const members=objects.filter(item=>group.ids.includes(item.id)&&item.id!==id);
 if(!members.length)return {x:extracted.x,y:extracted.y};
 const frame=stackFrame(members,false);if(!frame)return null;
 const hidden=new Set(stacks.filter(stack=>stack.collapsed).flatMap(stack=>stack.ids));
 const occupied=objects.filter(item=>item.id!==id&&!hidden.has(item.id)).map(item=>({x:item.x,y:item.y,w:item.width??230,h:item.height??82}));
 for(const other of stacks){
  if(other.id===group.id)continue;
  const otherFrame=stackFrame(objects.filter(item=>other.ids.includes(item.id)),!!other.collapsed);
  if(otherFrame)occupied.push(otherFrame);
 }
 const x=frame.x+frame.w+20;let y=Math.min(...members.map(item=>item.y));
 const width=extracted.width??230,height=extracted.height??82;
 while(occupied.some(r=>x<r.x+r.w+20&&x+width+20>r.x&&y<r.y+r.h+20&&y+height+20>r.y))y+=120;
 return {x,y};
}

// Include the visible stack frame instead of the hidden positions of collapsed cards.
export function fitBoard(objects:BoardObject[],stacks:Stack[],width:number,height:number){
 const hidden=new Set(stacks.filter(s=>s.collapsed).flatMap(s=>s.ids));
 const boxes=objects.filter(o=>!hidden.has(o.id)).map(o=>({x:o.x,y:o.y,w:o.width??230,h:o.height??82}));
 for(const stack of stacks){const members=objects.filter(o=>stack.ids.includes(o.id));if(!members.length)continue;
  const box=stackFrame(members,!!stack.collapsed);if(box)boxes.push(box);
 }
 if(!boxes.length||width<=0||height<=0)return null;
 const left=Math.min(...boxes.map(b=>b.x)),top=Math.min(...boxes.map(b=>b.y));
 const right=Math.max(...boxes.map(b=>b.x+b.w)),bottom=Math.max(...boxes.map(b=>b.y+b.h));
 const insetLeft=Math.min(100,width*.2),pad=Math.min(40,width*.1,height*.1);
 const usableW=Math.max(1,width-insetLeft-pad),usableH=Math.max(1,height-2*pad);
 const zoom=Math.min(1.5,usableW/(right-left),usableH/(bottom-top));
 return {zoom,x:insetLeft+(usableW-(right-left)*zoom)/2-left*zoom,y:pad+(usableH-(bottom-top)*zoom)/2-top*zoom};
}
