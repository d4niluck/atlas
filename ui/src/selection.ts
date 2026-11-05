export type Rectangle = {x:number;y:number;width:number;height:number};
export type BoardObject = {id:string;x:number;y:number;width?:number;height?:number};

// Canvas coordinates are local to the viewport, before pan and zoom.
export function objectsInArea(objects:BoardObject[], start:{x:number;y:number}, end:{x:number;y:number}, offset:{x:number;y:number}, zoom:number):string[]{
 const left=(Math.min(start.x,end.x)-offset.x)/zoom;
 const right=(Math.max(start.x,end.x)-offset.x)/zoom;
 const top=(Math.min(start.y,end.y)-offset.y)/zoom;
 const bottom=(Math.max(start.y,end.y)-offset.y)/zoom;
 return objects.filter(o=>o.x<=right && o.x+(o.width??230)>=left && o.y<=bottom && o.y+(o.height??82)>=top).map(o=>o.id);
}
export function selectionRectangle(a:{x:number;y:number},b:{x:number;y:number}):Rectangle{
 return {x:Math.min(a.x,b.x),y:Math.min(a.y,b.y),width:Math.abs(a.x-b.x),height:Math.abs(a.y-b.y)};
}

// Свёрнутая группа заменяет геометрию скрытых карточек своей видимой рамкой.
export function objectsAndStacksInArea(objects:BoardObject[],stacks:{id:string;ids:string[];collapsed?:boolean}[],start:{x:number;y:number},end:{x:number;y:number},offset:{x:number;y:number},zoom:number):string[]{
 const collapsed=stacks.filter(s=>s.collapsed);
 const hidden=new Set(collapsed.flatMap(s=>s.ids));
 const chosen=objectsInArea(objects.filter(o=>!hidden.has(o.id)),start,end,offset,zoom);
 for(const stack of collapsed){
  const members=objects.filter(o=>stack.ids.includes(o.id));
  if(!members.length)continue;
  const box={id:stack.id,x:Math.min(...members.map(o=>o.x))-20,y:Math.min(...members.map(o=>o.y))-45,width:240,height:72};
  if(objectsInArea([box],start,end,offset,zoom).length)chosen.push(...members.map(o=>o.id));
 }
 return [...new Set(chosen)];
}

// Additive selection keeps existing objects when the user draws another area.
export function mergeSelection(current:string[], chosen:string[], additive:boolean):string[]{
 return additive ? [...new Set([...current,...chosen])] : [...new Set(chosen)];
}

type Stack = {id:string;title:string;ids:string[];collapsed?:boolean};
export function stackSelection(stacks:Stack[], selected:string[], id:string):Stack[]{
 const ids=[...new Set(selected)];
 if(ids.length<2)return stacks;
 const selectedSet=new Set(ids);
 return [...stacks.map(s=>({...s,ids:s.ids.filter(item=>!selectedSet.has(item))})).filter(s=>s.ids.length),{id,title:'Группа',ids}];
}
