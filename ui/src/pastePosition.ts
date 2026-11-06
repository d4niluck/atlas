type Box={x:number;y:number;w:number;h:number};
/** Prefer a visible area outside cards and stack frames; use least overlap if full. */
export function pastePosition(canvas: Pick<HTMLElement,'clientWidth'|'clientHeight'>|null, viewport:{offset:{x:number;y:number};zoom:number}, occupied:Box[]=[]){
 const {offset,zoom}=viewport, width=canvas?.clientWidth??400,height=canvas?.clientHeight??320;
 const cardW=230*zoom,cardH=82*zoom;
 const xs=[Math.max(0,(width-cardW)/2)],ys=[Math.max(0,(height-cardH)/2)];
 for(let i=0;i<=8;i++){xs.push(Math.max(0,(width-cardW)*i/8));ys.push(Math.max(0,(height-cardH)*i/8))}
 let best={x:xs[0],y:ys[0]},bestScore=Infinity;
 for(const x of xs)for(const y of ys){
  const world={x:(x-offset.x)/zoom,y:(y-offset.y)/zoom,w:230,h:82};
  let overlap=0;for(const r of occupied)overlap+=Math.max(0,Math.min(world.x+world.w,r.x+r.w)-Math.max(world.x,r.x))*Math.max(0,Math.min(world.y+world.h,r.y+r.h)-Math.max(world.y,r.y));
  const distance=Math.hypot(x-xs[0],y-ys[0]);const score=overlap*1000+distance;
  if(score<bestScore){bestScore=score;best={x,y}}
 }
 return {x:(best.x-offset.x)/zoom,y:(best.y-offset.y)/zoom};
}
