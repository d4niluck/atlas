type Stack={id:string;ids:string[];collapsed?:boolean};

export function stackLayers(stacks:Stack[],focused:string|null){
 const expanded=stacks.filter(stack=>!stack.collapsed);
 const active=expanded.find(stack=>stack.id===focused)??expanded.at(-1)??null;
 return {active:active?.id??null,grouped:new Set(expanded.flatMap(stack=>stack.ids)),foreground:new Set(active?.ids??[])};
}

export function stackDropPriority(stack:Stack,active:string|null){
 return stack.collapsed?0:stack.id===active?2:1;
}

export function isForegroundEdge(edge:{from:string;to:string},foreground:Set<string>){
 return foreground.has(edge.from)&&foreground.has(edge.to);
}

export function isCrossingEdge(edge:{from:string;to:string},foreground:Set<string>){
 return foreground.has(edge.from)!==foreground.has(edge.to);
}

export function stackFocusForObject(stacks:Stack[],objectId:string,previous:string|null){
 return stacks.find(stack=>stack.ids.includes(objectId))?.id??previous;
}

export function stackFocusAfterDrop(stacks:Stack[],target:string|undefined,previous:string|null){
 return stacks.find(stack=>stack.id===target&&!stack.collapsed)?.id??previous;
}


type Rect={x:number;y:number;w:number;h:number};

// All rectangles use board coordinates, so panning and zooming do not affect overlap.
export function stackNeedsBlur(stackId:string,stacks:Stack[],cards:{id:string;rect:Rect}[],stackRects:Map<string,Rect>,activeId:string|null){
 const target=stacks.find(stack=>stack.id===stackId);
 const frame=stackRects.get(stackId);
 if(!target||target.collapsed||!frame)return false;
 const overlaps=(rect:Rect)=>frame.x<rect.x+rect.w&&frame.x+frame.w>rect.x&&frame.y<rect.y+rect.h&&frame.y+frame.h>rect.y;
 const own=new Set(target.ids);
 const grouped=new Set(stacks.filter(stack=>!stack.collapsed).flatMap(stack=>stack.ids));
 if(cards.some(card=>!own.has(card.id)&&(stackId===activeId||!grouped.has(card.id))&&overlaps(card.rect)))return true;
 return stacks.some(stack=>stack.collapsed&&stack.id!==stackId&&!!stackRects.get(stack.id)&&overlaps(stackRects.get(stack.id)!));
}
