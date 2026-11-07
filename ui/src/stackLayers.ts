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
