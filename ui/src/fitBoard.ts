import type {BoardObject} from './selection';
type Stack={id:string;ids:string[];collapsed?:boolean};

// Include the visible stack frame instead of the hidden positions of collapsed cards.
export function fitBoard(objects:BoardObject[],stacks:Stack[],width:number,height:number){
 const hidden=new Set(stacks.filter(s=>s.collapsed).flatMap(s=>s.ids));
 const boxes=objects.filter(o=>!hidden.has(o.id)).map(o=>({x:o.x,y:o.y,w:o.width??230,h:o.height??82}));
 for(const stack of stacks){const members=objects.filter(o=>stack.ids.includes(o.id));if(!members.length)continue;
  const x=Math.min(...members.map(o=>o.x))-20,y=Math.min(...members.map(o=>o.y))-45;
  boxes.push({x,y,w:stack.collapsed?240:Math.max(...members.map(o=>o.x))+250-x,h:stack.collapsed?72:Math.max(...members.map(o=>o.y))+100-y});
 }
 if(!boxes.length||width<=0||height<=0)return null;
 const left=Math.min(...boxes.map(b=>b.x)),top=Math.min(...boxes.map(b=>b.y));
 const right=Math.max(...boxes.map(b=>b.x+b.w)),bottom=Math.max(...boxes.map(b=>b.y+b.h));
 const insetLeft=Math.min(100,width*.2),pad=Math.min(40,width*.1,height*.1);
 const usableW=Math.max(1,width-insetLeft-pad),usableH=Math.max(1,height-2*pad);
 const zoom=Math.min(1.5,usableW/(right-left),usableH/(bottom-top));
 return {zoom,x:insetLeft+(usableW-(right-left)*zoom)/2-left*zoom,y:pad+(usableH-(bottom-top)*zoom)/2-top*zoom};
}
