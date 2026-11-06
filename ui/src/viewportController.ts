export type Viewport={offset:{x:number;y:number};zoom:number};
type Timer=ReturnType<typeof setTimeout>;
export function createViewport(initial:Viewport,paint:(v:Viewport)=>void,commit:(v:Viewport)=>void,schedule:(f:()=>void,n:number)=>Timer=setTimeout,cancel:(t:Timer)=>void=clearTimeout,motion:(moving:boolean)=>void=()=>{}){
 let value:Viewport={offset:{...initial.offset},zoom:initial.zoom},timer:Timer|undefined,moving=false,pointerActive=false;
 function setMoving(next:boolean){if(moving!==next){moving=next;motion(next)}}
 const copy=():Viewport=>({offset:{...value.offset},zoom:value.zoom});
 function clear(){if(timer!==undefined){cancel(timer);timer=undefined}}
 function publish(){paint(copy())}
 function finish(){clear();pointerActive=false;setMoving(false);commit(copy())}
 function gesture(next:Viewport){clear();setMoving(true);value=next;publish()}
 return {get:copy,beginPointer(){pointerActive=true},pan(dx:number,dy:number){gesture({...value,offset:{x:value.offset.x+dx,y:value.offset.y+dy}})},zoomAt(point:{x:number;y:number},factor:number){const old=value.zoom,next=Math.max(.001,Math.min(5,old*factor));gesture({zoom:next,offset:{x:point.x-(point.x-value.offset.x)*next/old,y:point.y-(point.y-value.offset.y)*next/old}})},movePointer(origin:{x:number;y:number},dx:number,dy:number){gesture({...value,offset:{x:origin.x+dx,y:origin.y+dy}})},set(next:Viewport){clear();setMoving(false);value={offset:{...next.offset},zoom:next.zoom};publish();commit(copy())},wheelEnd(){clear();timer=schedule(()=>{timer=undefined;if(!pointerActive)setMoving(false);commit(copy())},120)},finish,dispose(){clear();pointerActive=false;setMoving(false)}};
}
