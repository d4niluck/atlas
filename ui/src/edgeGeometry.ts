export type EdgeBox={x:number;y:number;width:number;height:number};

// Intersect the centre-to-centre ray with a card border, so arrows remain visible.
export function edgeGeometry(a:EdgeBox,b:EdgeBox):string{
 const ax=a.x+a.width/2,ay=a.y+a.height/2,bx=b.x+b.width/2,by=b.y+b.height/2;
 const dx=bx-ax,dy=by-ay;if(!dx&&!dy)return '';
 const scale=(box:EdgeBox)=>1/Math.max(Math.abs(dx)/(box.width/2),Math.abs(dy)/(box.height/2));
 const s=scale(a),t=scale(b),x1=ax+dx*s,y1=ay+dy*s,x2=bx-dx*t,y2=by-dy*t;
 const bend=Math.max(30,Math.min(120,Math.hypot(x2-x1,y2-y1)*.35));
 if(Math.abs(dx)>=Math.abs(dy)){const k=Math.sign(dx)*bend;return `M ${x1} ${y1} C ${x1+k} ${y1}, ${x2-k} ${y2}, ${x2} ${y2}`}
 const k=Math.sign(dy)*bend;return `M ${x1} ${y1} C ${x1} ${y1+k}, ${x2} ${y2-k}, ${x2} ${y2}`;
}
